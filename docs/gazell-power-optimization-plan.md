# Gazell 2.4GHz Power Optimization Plan

> Date: 2026-03-14
> Status: Layer 1 & 2 planned, Layer 3 reserved for future work
> Context: Phase 3 multi-pipe Gazell split complete and hardware-verified

## Current Power Profile

The Gazell 2.4GHz split keyboard implementation prioritizes correctness and
simplicity. As a result, several power-inefficient patterns exist that should
be addressed before battery-powered deployment.

### Estimated Idle Current Budget (Before Optimization)

| Component | Current | Notes |
|-----------|---------|-------|
| nRF52840 CPU (no WFE in gz_send) | 3.0 mA | Busy-wait `__NOP()` loop |
| HFCLK 32MHz crystal (always on) | 0.25 mA | Started at boot, never stopped |
| RADIO (Gazell device, 50ms heartbeat) | ~0.5 mA | 20Hz TX duty cycle |
| LDO regulation overhead | ~1.0 mA | If DCDC not enabled |
| PMW3610 sensor (idle, hw power-save) | ~1.0 mA | Sensor auto-downshifts |
| **Total estimated idle** | **~5.75 mA** | |

With a typical 110mAh keyboard battery: **~19 hours idle**.

---

## Layer 1: Quick Wins

Minimal code changes, significant power reduction.
**Target: ~2-3 mA idle.**

### 1.1 Replace `__NOP()` with `__WFE()` in C Shim

**File:** `rmk-gazell-sys/c/gazell_shim.c`

**Locations:**
- Line 297: `gz_send()` busy-wait loop
- Line 215-217: `gz_set_mode()` disable-wait loop

**Change:**
```c
// Before:
__NOP();

// After:
__WFE();
```

**Rationale:** `__WFE()` (Wait For Event) puts the CPU into low-power sleep
until an interrupt fires. The Gazell radio ISR (RADIO_IRQHandler) generates
the event that wakes the CPU. Power during each TX drops from ~3mA to ~0.5uA.

### 1.2 Increase Default Heartbeat from 50ms to 200ms

**File:** `rmk/src/wireless/config.rs`

**Change:**
```rust
// Before:
heartbeat_interval_ms: 50,

// After:
heartbeat_interval_ms: 200,
```

**Rationale:** Heartbeat drives `gz_send()` calls even when idle. 50ms (20Hz)
is unnecessarily aggressive. 200ms (5Hz) reduces radio wake events by 4x while
staying well under the 1000ms `PERIPHERAL_TIMEOUT_MS` disconnect threshold.

### ~~1.3 Stop HFCLK After Gazell Init~~ (REVERTED)

**Status:** Attempted and reverted after hardware verification failure.

**Root cause:** Nordic's precompiled Gazell library does NOT manage HFXO
(32MHz crystal) internally. It expects the application to keep HFXO running.
Stopping HFXO after `gz_init_default()` causes the RADIO peripheral to fall
back to HFINT (64MHz internal RC oscillator), which has insufficient frequency
accuracy for reliable 2.4GHz communication. This causes:

- Intermittent `gz_send()` failures (RADIO frequency drift → dongle can't receive)
- Right hand more affected than left because PMW3610's 2s startup grace means
  the first `gz_send()` happens well after HFXO has stopped

**Clarification on TIMER2:** Gazell's TIMER2 uses PCLK16M which works with
either HFINT or HFXO — TIMER2 itself does NOT require HFXO. The actual blocker
is the RADIO peripheral needing HFXO for frequency accuracy.

**Lesson learned:** Simply stopping HFXO globally breaks Gazell because the
precompiled library doesn't re-request it before radio events. The correct
low-power approach is to wrap each `gz_send()`/`gz_recv()` with HFXO
start/stop at the C shim level (see Layer 3.5 below).

**Future possibility (Layer 3.5):** Add HFXO management to `gz_send()`:
```c
// In gazell_shim.c gz_send():
NRF_CLOCK->TASKS_HFCLKSTART = 1;
while (NRF_CLOCK->EVENTS_HFCLKSTARTED == 0) { __WFE(); }  // ~0.25ms
// ... do TX ...
// After TX complete:
NRF_CLOCK->TASKS_HFCLKSTOP = 1;
```
This would save ~0.25-0.5mA between radio events at the cost of ~0.25ms
additional latency per transmission. Combined with 500ms idle heartbeat,
HFXO would only run for ~1ms per second (0.2% duty cycle).

### Layer 1 Verification

| Test | Method | Pass Criteria |
|------|--------|---------------|
| Compile | `cargo build --release --bin central/peripheral/peripheral_right` | All 3 binaries pass |
| Unit tests | `cargo test --no-default-features --features "std,split,wireless_gazell"` | All tests pass |
| Key input | Flash all 3, type on both hands | No dropped keys, same behavior as before |
| Trackball | Move trackball after flash | Smooth cursor movement |
| ~~HFCLK regression~~ | ~~Flash peripheral, verify communication~~ | **REVERTED: TIMER2 needs HFCLK** |
| Heartbeat interval | Idle 2s then press key | Key response within 200ms (imperceptible) |
| Disconnect detection | Power off right hand while holding a key | Key released within 1s |

---

## Layer 2: Adaptive Heartbeat + Idle Mode

Peripheral tracks activity and reduces radio duty when idle.
**Target: ~0.5-1 mA idle.**

### 2.1 Add Idle Configuration to GazellConfig

**File:** `rmk/src/wireless/config.rs`

**New fields:**
```rust
pub struct GazellConfig {
    // ... existing fields ...

    /// Heartbeat interval when idle (no key/pointing activity).
    /// Longer interval saves power at the cost of slightly slower
    /// first-key response after idle.
    /// Default: 500 ms
    pub idle_heartbeat_interval_ms: u16,

    /// Time without any key/pointing activity before entering idle mode.
    /// Default: 5000 ms (5 seconds)
    pub idle_timeout_ms: u16,
}
```

**Defaults:**
```rust
idle_heartbeat_interval_ms: 500,  // 2Hz when idle
idle_timeout_ms: 5000,            // 5s to enter idle
```

### 2.2 GazellPeripheralDriver Adaptive Heartbeat

**File:** `rmk/src/split/gazell.rs`

**New fields in `GazellPeripheralDriver`:**
```rust
idle: bool,
last_activity: Instant,
idle_heartbeat_interval_ms: u16,
idle_timeout_ms: u16,
```

**Behavior:**

1. **`write()` (actual message sent):** Reset activity timer
   ```rust
   self.idle = false;
   self.last_activity = Instant::now();
   ```

2. **`read()` (heartbeat loop):** Check idle transition
   ```rust
   if !self.idle && self.last_activity.elapsed().as_millis() > self.idle_timeout_ms as u64 {
       self.idle = true;
   }
   ```

3. **Heartbeat interval selection:**
   ```rust
   let interval = if self.idle {
       self.idle_heartbeat_interval_ms
   } else {
       self.heartbeat_interval_ms
   };
   ```

4. **Yield interval:** Longer sleep when idle
   ```rust
   Timer::after_millis(if self.idle { 50 } else { 5 }).await;
   ```

### 2.3 Central Hub Polling Backoff

**File:** `rmk/src/split/gazell.rs`

**Current:** Fixed 1ms polling in `run_gazell_central_hub()`.

**Change:** When no data received, increase to 5ms:
```rust
if ret == sys::GZ_OK && len > 0 {
    // ... process data ...
    Timer::after_millis(1).await;  // Fast poll after data
} else {
    Timer::after_millis(5).await;  // Slower poll when idle
}
```

**Note:** The dongle is USB-powered, so this is a CPU efficiency improvement
rather than a power saving. It reduces unnecessary wakeups from 1000Hz to 200Hz.

### Layer 2 Verification

| Test | Method | Pass Criteria |
|------|--------|---------------|
| Compile | Same as Layer 1 | All 3 binaries pass |
| Unit tests | Same as Layer 1, plus new idle-related tests if added | All tests pass |
| Active→Idle transition | Type, stop for 5s, type again | First key after idle responds within 500ms |
| Idle heartbeat frequency | Stop typing >5s, then press key | Response delay ≤500ms (one idle heartbeat period) |
| Trackball idle recovery | Stop moving >5s, then move | Cursor movement resumes smoothly |
| Disconnect still works | Power off peripheral after 10s idle | Keys released within 1s (500ms < 1000ms timeout) |
| Sustained typing | Fast typing for 30s | No dropped keys, no extra latency |
| Active mode performance | Compare typing feel with pre-optimization | No perceptible difference |

---

## Layer 3: Deep Power Management (Reserved — Not Implemented)

These optimizations require architecture-level changes and will be implemented
in a future PR.

### 3.1 Radio Shutdown on Deep Idle

**Concept:** After extended idle (e.g., 5 minutes with no activity), call
`gz_deinit()` to fully disable the radio peripheral. Wake via GPIO interrupt
on matrix pins, then `gz_init_default()` to re-establish Gazell link.

**Key details:**
- `gz_deinit()` already exists in C shim but is never called
- Re-init latency: ~1-5ms (imperceptible)
- Power saving: radio from ~0.5mA average to 0

**Estimated effect:** Idle current drops from ~0.5-1mA (Layer 2) to ~0.01mA.

### 3.2 System OFF Deep Sleep

**Concept:** Port BLE's sleep manager architecture to Gazell. After very
long idle (e.g., 30 minutes), enter nRF52840 System OFF mode (~0.3uA).
Wake via GPIO interrupt on matrix row/col pins.

**Key details:**
- BLE sleep manager lives in `split/ble/central.rs:504-591`
- Uses `SLEEPING_STATE: AtomicBool` and `CENTRAL_SLEEP: Signal<bool>`
- For Gazell: would need to lift these to a shared location
- Matrix `wait_for_key()` (`matrix.rs:520`) already supports GPIO wake

**Estimated effect:** Ultra-deep idle at ~0.3uA. Battery life: months.

### 3.3 PMW3610 Power-Down Command

**Concept:** When entering deep idle, send a power-down register write to
PMW3610 sensor. Currently the sensor auto-downshifts (run→rest1→rest2→rest3)
but never fully powers down.

**Key details:**
- PMW3610 has `PERFORMANCE_FMODE_FORCE_AWAKE` and `PERFORMANCE_FMODE_NORMAL`
  registers (`pmw3610.rs:72-74`)
- Could add a `PERFORMANCE_FMODE_SHUTDOWN` or use SPI CS deassert
- On wake: re-run `configure()` sequence

**Estimated effect:** Sensor from ~1mA idle to ~0.01mA.

### 3.4 ISR-Driven Central Hub

**Concept:** Replace the 1-5ms polling loop in `run_gazell_central_hub()` with
an interrupt-driven approach. The `nrf_gzll_host_rx_data_ready()` C callback
would signal an Embassy `Signal`, and the hub would `await` that signal.

**Key details:**
- C callback already exists (`gazell_shim.c:84`)
- Need to add `Signal` export from Rust and `extern "C"` bridge
- Eliminates all polling overhead on dongle

**Estimated effect:** Dongle CPU utilization from ~5% to ~0.1%.

### Layer 3 Estimated Power Profile

| State | Current | Battery Life (110mAh) |
|-------|---------|----------------------|
| Active typing | ~4 mA | ~27 hours |
| Idle (Layer 2, radio active) | ~0.5-1 mA | 5-9 days |
| Deep idle (radio off) | ~0.01 mA | ~1 year |
| System OFF | ~0.3 uA | ~40 years (battery self-discharge limited) |

---

## Reference: BLE Power Management Architecture

For comparison, BLE's power management stack (relevant for Layer 3 porting):

```
keyboard.toml
  └─ split_central_sleep_timeout_seconds: u32

build.rs → constants.rs
  └─ SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS

Global Signals
  ├─ CENTRAL_SLEEP: Signal<bool>     (sleep/wake control)
  ├─ SLEEPING_STATE: AtomicBool      (current state query)
  ├─ LAST_KEY_TIMESTAMP: Signal<u32> (activity tracking)
  └─ KEYBOARD_REPORT_CHANNEL         (keypress wake trigger)

Sleep Manager Task (central.rs:506)
  ├─ Timeout → sleep (relax BLE conn params)
  ├─ Signal(false) → wake (restore conn params)
  └─ Publishes SleepStateEvent

Activity Sources
  ├─ Peripheral key/pointing messages → update_activity_time()
  ├─ GATT CCCD writes (macOS wakeup)
  ├─ HID Control Point (host suspend/resume)
  └─ KEYBOARD_REPORT_CHANNEL (any keypress)

Sleep-Aware Consumers
  ├─ Battery service: skips reports while sleeping
  ├─ BLE conn params: relaxed interval + high latency
  └─ Matrix scanner: GPIO wait_for_key() → WFI
```

Gazell equivalent mapping for Layer 3:

| BLE Concept | Gazell Equivalent |
|-------------|-------------------|
| Relax conn params | Increase heartbeat interval (Layer 2) |
| Advertising timeout → sleep | Idle timeout → `gz_deinit()` (Layer 3.1) |
| System OFF + GPIO wake | Same mechanism, portable (Layer 3.2) |
| `update_activity_time()` | `self.idle = false` in `write()` (Layer 2) |

---

## Version History

- 2026-03-14 v1: Initial plan based on BLE power management analysis
- 2026-03-14 v2: Layer 1.3 (HFCLK stop) reverted — precompiled Gazell library
  does not manage HFXO internally; RADIO needs HFXO for frequency accuracy.
  Correct fix is per-TX HFXO management in C shim (deferred to Layer 3.5).
  Layer 1.1, 1.2, and full Layer 2 implemented and hardware-verified.
