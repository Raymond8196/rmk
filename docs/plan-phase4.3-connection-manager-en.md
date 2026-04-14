# Phase 4.3: ConnectionManager — Detailed Implementation Plan

**Status**: Draft (awaiting hardware verification of Phase 4.2)
**Branch**: `feat/gazell-rebase`
**Prerequisites**: Phase 4.2 HW verification (BLE pause/resume with Gazell switching)

## 1. Overview

Implement a unified `ConnectionManager` that dynamically switches between Gazell 2.4GHz and BLE connections on nRF52840, triggered by user keypresses.

## 2. State Machine

```
                  ┌─────────┐
        boot ───► │ Gazell  │ ◄── default (dongle mode)
                  └────┬────┘
                       │ SwitchToBle
                       ▼
                  ┌─────────┐
                  │Switching│ ─── gz_deinit()
                  │ (BLE)   │ ─── settle 200ms
                  └────┬────┘ ─── switch_to_ble()
                       │      ─── start advertising
                       ▼
                  ┌─────────┐
                  │   BLE   │ ─── advertising → connected
                  └────┬────┘
                       │ SwitchToGazell
                       ▼
                  ┌─────────┐
                  │Switching│ ─── stop advertising
                  │(Gazell) │ ─── settle 200ms
                  └────┬────┘ ─── switch_to_gazell()
                       │      ─── gz_init_default(1)
                       ▼
                  ┌─────────┐
                  │ Gazell  │ ◄── loop
                  └─────────┘
```

## 3. Implementation Steps

### Step 1: Config & Feature Flag

**Files**: `rmk-config/src/lib.rs`, `rmk/Cargo.toml`

- Add `gazell_ble` to allowed connection types in `SplitConfig.connection` validation
- Add `wireless_gazell_ble` feature flag combining `wireless_gazell` + `_ble`
- Ensure `nrf-mpsl` critical-section-impl is used (not cortex-m's)

### Step 2: Keycode for Connection Switching

**Files**: `rmk-types/src/lib.rs`, `rmk/src/keycode.rs` (or equivalent)

Add keycode variants:
```rust
// In KeyAction or similar
SwitchToGazell,   // Switch radio to Gazell 2.4GHz
SwitchToBle,      // Switch radio to BLE
ToggleConnection, // Toggle between Gazell and BLE
```

Wire into keyboard task: when this keycode is processed, send `SwitchEvent` to ConnectionManager channel.

### Step 3: ConnectionManager Task

**File**: `rmk/src/wireless/connection_manager.rs`

```rust
pub async fn run_connection_manager(
    gazell_config: GazellConfig,
    num_pipes: usize,
    // BLE stack references (Peripheral, Runner from trouble-host)
    // Passed from codegen
) {
    let mut state = ManagerState::GazellActive;
    let mut retry_count = 0u8;

    loop {
        match state {
            ManagerState::GazellActive => {
                // Gazell hub + pipe managers already running
                // Wait for switch event
                let event = switch_channel.receive().await;
                if matches!(event, SwitchEvent::ToBle | SwitchEvent::Fallback) {
                    state = ManagerState::SwitchingToBle;
                    // Signal Gazell tasks to stop
                    GAZELL_HUB_CANCEL.signal(());
                }
            }
            ManagerState::SwitchingToBle => {
                // 1. Wait for Gazell hub to exit (poison pill propagation)
                // 2. gz_deinit()
                // 3. Settle delay (200ms)
                // 4. switch_to_ble()
                // 5. Start BLE advertising
                // 6. On success → BleActive, on failure → retry or Error
                state = ManagerState::BleActive;
            }
            ManagerState::BleActive => {
                // BLE advertising/connected
                // Wait for switch event
                let event = switch_channel.receive().await;
                if matches!(event, SwitchEvent::ToGazell | SwitchEvent::Fallback) {
                    state = ManagerState::SwitchingToGazell;
                    // Stop BLE advertising
                }
            }
            ManagerState::SwitchingToGazell => {
                // 1. Stop BLE advertising
                // 2. Settle delay (200ms)
                // 3. switch_to_gazell()
                // 4. gz_init_default(1)
                // 5. Restart Gazell hub + pipe managers
                // 6. On success → GazellActive, on failure → retry or Error
                state = ManagerState::GazellActive;
            }
            ManagerState::Error => {
                // Stay in last known good mode
                // Log error, wait for manual reset or timeout
                Timer::after_secs(30).await;
                state = ManagerState::GazellActive; // Reset to default
            }
        }
    }
}
```

### Step 4: Codegen Integration

**Files**: `rmk-macro/src/codegen/entry.rs`, `rmk-macro/src/codegen/split/central.rs`

When `connection == "gazell_ble"`:
1. Generate both MPSL/SDC init AND Gazell init code
2. Spawn `run_connection_manager()` as the main task instead of separate Gazell/BLE tasks
3. Generate BLE stack setup (same as BLE central) but don't start scanning immediately
4. Generate Gazell setup but don't start hub immediately
5. ConnectionManager orchestrates both

### Step 5: Peripheral (Keyboard Half) Changes

**File**: `rmk-macro/src/codegen/split/peripheral.rs`

For `connection == "gazell_ble"` peripherals:
- Start in Gazell mode (default)
- Listen for mode switch commands from central via ACK payloads
- On `SwitchToBle` command: stop Gazell, switch RADIO, start BLE advertising
- On `SwitchToGazell` command: stop BLE, switch RADIO, restart Gazell

### Step 6: BLE Stack Lifecycle Management

Key challenge: BLE trouble-host `Runner` and `Host` objects need to stay alive across switches.

Pattern (from Phase 4.2 PoC):
```rust
// Build Host ONCE — Runner + Peripheral stay alive across switches
let Host { mut peripheral, mut runner, .. } = stack.build();

// Runner runs forever (errors suppressed during Gazell mode)
spawn(runner_loop(runner));

// ConnectionManager controls advertising:
// - Gazell mode: don't advertise, Runner idles
// - BLE mode: advertise, Runner processes connections
```

## 4. Error Recovery

| Scenario | Recovery |
|----------|----------|
| BLE advertise fails after switch | Retry 3x with 500ms backoff, then fall back to Gazell |
| Gazell init fails after switch | Retry 3x with 500ms backoff, then fall back to BLE |
| Runner errors during Gazell mode | Suppress (expected — no RADIO events routed to MPSL) |
| Switching timeout (>2s) | Force to Error state, reset after 30s |
| Both protocols fail | Stay in Error state, require hardware reset |

## 5. Data Flow

```
Keypress (SwitchToBle)
    → Keyboard task processes keycode
    → Sends SwitchEvent::ToBle via Channel
    → ConnectionManager receives event
    → Signals GAZELL_HUB_CANCEL
    → Hub exits, sends poison pills to pipe managers
    → ConnectionManager: gz_deinit() + settle + switch_to_ble()
    → ConnectionManager: peripheral.advertise()
    → BLE connected → keyboard traffic via BLE GATT
```

## 6. Resource Requirements

| Resource | Estimate |
|----------|----------|
| RAM (stack + heapless) | ~8KB additional (BLE HostResources + channels) |
| Flash | ~15KB additional (BLE SDC + trouble-host + ConnectionManager) |
| Binary size | ~100KB total (Gazell + BLE + USB CDC) |

## 7. Hardware Verification Test Plan

### Test 1: Basic Gazell → BLE Switch
1. Boot in Gazell mode, verify keypress → USB HID works
2. Press `SwitchToBle` key
3. Verify Gazell deinit, RADIO switch, BLE advertising starts
4. Verify nRF Connect can find the keyboard
5. Verify no crash or hang

### Test 2: BLE → Gazell Switch
1. From BLE advertising, press `SwitchToGazell`
2. Verify BLE adv stops, RADIO switch, Gazell init
3. Verify keypress → USB HID works again
4. Verify no crash or hang

### Test 3: Rapid Switching
1. Alternate `SwitchToBle` / `SwitchToGazell` rapidly (5x)
2. Verify no memory corruption, no deadlock
3. Verify keyboard ends in the last requested mode

### Test 4: Error Recovery
1. Force BLE advertise failure (e.g., disable antenna)
2. Verify fallback to Gazell after retries
3. Verify keyboard continues to work in Gazell mode

## 8. Timeline

| Step | Description | Depends On |
|------|-------------|------------|
| 1 | Config & feature flags | Phase 4.2 HW verified |
| 2 | Keycode for switching | Step 1 |
| 3 | ConnectionManager task | Steps 1, 2 |
| 4 | Codegen integration | Step 3 |
| 5 | Peripheral changes | Step 4 |
| 6 | BLE lifecycle management | Step 3 |
| HW | Hardware verification | Steps 1-6 |

## 9. Open Questions

1. **Should the dongle remember the last used mode across reboots?** — Requires storage integration.
2. **Should peripheral halves follow the central's mode automatically?** — Via ACK payload command, or independent?
3. **BLE pairing during Gazell mode?** — User might want to pair new BLE device while using Gazell. Requires partial BLE stack active.
4. **USB behavior during switching?** — USB HID should remain active throughout (it's on a separate endpoint, not RADIO).
5. **Power consumption budget?** — BLE + Gazell stacks both in RAM means ~1mA additional idle current vs Gazell-only.

## 10. Changelog

- 2026-04-14: Initial draft, interface design in `rmk/src/wireless/connection_manager.rs`
