# BLE ↔ Gazell Runtime Switching Risk Analysis

> Analysis of implementing runtime hot-switching between BLE and Gazell 2.4G protocols on nRF52840.

## Background

### Hardware Constraint

nRF52840 has only **one RADIO peripheral**. BLE and Gazell cannot run simultaneously.

```
┌─────────────────────────────────────┐
│           nRF52840                   │
│  ┌─────────┐                        │
│  │  RADIO  │ ← Only ONE!            │
│  └────┬────┘                        │
│       │                              │
│  ┌────┴────┬────────────┐           │
│  │   BLE   │   Gazell   │           │
│  │ (one at a time)       │           │
│  └─────────┴────────────┘           │
└─────────────────────────────────────┘
```

### Current Architecture

| Aspect | BLE (trouble-host) | Gazell (Nordic SDK) |
|--------|-------------------|---------------------|
| Stack Control | `Stack::build()` init | `gz_init()` / `gz_deinit()` |
| Destroy Mechanism | ❌ None (static lifetime design) | ✅ Has `gz_deinit()` |
| RADIO Interrupt | `nrf_sdc::mpsl::HighPrioInterruptHandler` | C library `RADIO_IRQHandler` |
| RAM Usage | ~10KB+ | ~1KB |

---

## Proposed Solution: Runtime Dynamic Switching (Option C)

### Switching Flow

```
User triggers switch (e.g., Fn+Key)
        ↓
    Save state to Flash
        ↓
    Disconnect BLE / Stop Gazell
        ↓
    Rebind RADIO interrupt
        ↓
    Initialize new protocol
        ↓
    Restore connection
```

---

## Risk Analysis

### Risk A: BLE Stack Lacks Destruction Mechanism (HIGH)

**Problem**: `trouble-host` is designed with static lifetime resources:

```rust
// trouble-host design pattern:
let mut host_resources: HostResources<'static, ...> = HostResources::new();
let stack: Stack<'static, ...> = trouble_host::new(controller, &mut host_resources);

// Stack and HostResources have no stop()/destroy() methods
```

**Impact**: After switching to Gazell, BLE static resources cannot be released. RAM cannot be reused.

**Mitigation Options**:
1. Wrap `trouble-host` in a "paused" state (disable advertising, stop runner)
2. Modify `trouble-host` upstream to support destruction
3. Accept RAM waste (~10KB) and only have one protocol active at a time

**Workaround**: Test if BLE stack can be "silenced" without destruction:

```rust
// Pseudo-code
async fn pause_ble(stack: &Stack) {
    // Stop advertising
    stack.stop_advertising();
    // Stop runner task (need to track and cancel the task)
    runner_cancel_token.cancel();
    // RADIO interrupt will be idle
}
```

---

### Risk B: RADIO Interrupt Rebinding (MEDIUM)

**Problem**: `embassy-nrf` binds interrupts at compile time:

```rust
// Current approach - compile-time binding
#[pac::interrupt]
fn RADIO() {
    // Fixed at compile time, cannot change at runtime
}
```

**Solution**: Dynamic interrupt dispatcher:

```rust
// Runtime dispatch approach
static RADIO_HANDLER: AtomicU8 = AtomicU8::new(0); // 0=BLE, 1=Gazell

#[pac::interrupt]
fn RADIO() {
    match RADIO_HANDLER.load(Ordering::Relaxed) {
        0 => unsafe { 
            // BLE handler
            nrf_sdc::mpsl::HighPrioInterruptHandler::on_radio() 
        },
        1 => unsafe { 
            // Gazell handler (from C library)
            RADIO_IRQHandler() 
        },
        _ => {}
    }
}

// Switch function
fn switch_radio_handler(mode: u8) {
    // 1. Disable current handler
    interrupt::RADIO.disable();
    
    // 2. Update handler selector
    RADIO_HANDLER.store(mode, Ordering::SeqCst);
    
    // 3. Clear pending interrupts
    interrupt::RADIO.clear_pend();
    
    // 4. Re-enable
    interrupt::RADIO.enable();
}
```

**Impact**: Requires modifying RMK's interrupt binding code in `rmk-macro/src/codegen/`.

---

### Risk C: Race Conditions During Switching (MEDIUM)

**Problem**: User may press keys during the switch window:

```
Timeline:
BLE advertising → User triggers switch → BLE disconnect → Gazell start
       ↑                                    ↓
       └──── User pressing keys ───────────┘
```

**Impact**: Keys pressed during switch may be lost or generate abnormal reports.

**Mitigation**:

```rust
struct ConnectionSwitcher {
    switching: AtomicBool,
    pending_events: Vec<KeyboardEvent, 16>,
}

impl ConnectionSwitcher {
    async fn switch(&mut self, target: ConnectionType) {
        // 1. Mark as switching - block new key processing
        self.switching.store(true, Ordering::SeqCst);
        
        // 2. Flush pending reports
        self.flush_pending_reports().await;
        
        // 3. Perform switch
        // ...
        
        // 4. Clear flag
        self.switching.store(false, Ordering::SeqCst);
    }
}

// In keyboard processing:
if switcher.switching.load() {
    // Buffer events during switch
    pending_events.push(event);
    return;
}
```

---

### Risk D: Memory Layout Conflicts (LOW)

**Problem**: Both stacks require static memory:

```rust
// BLE resources (always allocated)
static mut HOST_RESOURCES: HostResources<...>; // ~10KB

// Gazell resources (always allocated)
static gz_state: GzState; // ~100 bytes

// Total: ~10KB "wasted" when one protocol is inactive
```

**Impact**: Acceptable on nRF52840 (256KB RAM), but reduces available memory for other features.

**Mitigation**: Use `#[cfg(feature = "...")]` to exclude unused protocol's statics at compile time if runtime switching is not needed.

---

## Risk Matrix

| Risk | Severity | Solvability | Effort | Priority |
|------|----------|-------------|--------|----------|
| BLE no deinit | High | Medium | Large | P0 |
| RADIO rebind | Medium | High | Medium | P1 |
| Switch race | Medium | High | Medium | P2 |
| RAM waste | Low | N/A | None | P3 |

---

## Implementation Roadmap

### Phase 1: Verify RADIO Switching (Proof of Concept)

- [ ] Implement dynamic RADIO interrupt dispatcher
- [ ] Test BLE can start → stop → restart independently
- [ ] Test Gazell can init → deinit → reinit independently
- [ ] Verify both can alternate without hardware reset

**Validation**: Can switch 10+ times without crash or RADIO malfunction.

### Phase 2: BLE Stack Encapsulation

- [ ] Wrap `trouble-host` as restartable service
- [ ] Implement "pseudo-destruction" (disable advertising + stop runner)
- [ ] Test BLE runner can be cancelled and restarted
- [ ] Verify RAM state after stop (if re-initialization works)

**Key Question**: Can `trouble_host::new()` be called multiple times?

### Phase 3: Unified Connection Manager

- [ ] Implement `ConnectionManager` trait
- [ ] Add keycodes for switching: `KeyCode::SwitchToBle`, `KeyCode::SwitchToGazell`
- [ ] Persist connection type to Flash
- [ ] Event system integration for UI feedback

### Phase 4: Production Hardening

- [ ] Handle edge cases (switch during active connection)
- [ ] Add LED indicator for current mode
- [ ] Timeout handling if switch fails
- [ ] User documentation

---

## Open Questions

1. **trouble-host reinitialization**: Can `trouble_host::new()` be called multiple times with the same `HostResources`?
   - Need: Source code investigation or experimental validation

2. **Switch time requirement**: How long can the switch take?
   - < 100ms: User won't notice
   - 100ms - 1s: Acceptable with UI feedback
   - > 1s: May need different approach

3. **RAM sufficiency**: Does BLE + Gazell static resources fit in RAM?
   - nRF52840: 256KB RAM
   - BLE: ~10KB
   - Gazell: ~1KB
   - Remaining: Should be sufficient

---

## Alternative: Option B (Dual-Slot Boot)

If Option C proves too complex, fallback to Option B:

```
Flash Layout:
┌────────────────────────────────────┐ 0x0000
│  Bootloader (mode selector)        │
├────────────────────────────────────┤ 0x1000
│  BLE Firmware                      │
│  - Full BLE stack                  │
│  - CONNECTION_TYPE = Ble           │
├────────────────────────────────────┤ 0x40000
│  Gazell Firmware                   │
│  - Gazell protocol                 │
│  - CONNECTION_TYPE = Gazell        │
└────────────────────────────────────┘

Switch Flow:
1. User presses switch key
2. Save target mode to UICR/Flash
3. Trigger NVIC_SystemReset()
4. Bootloader reads saved mode
5. Jump to corresponding firmware address
```

**Trade-off**: Requires reboot (~1-2s), but no runtime complexity.

---

## References

- `rmk/src/ble/mod.rs` - BLE initialization
- `rmk/src/wireless/gazell.rs` - Gazell wrapper
- `rmk-gazell-sys/c/gazell_shim.c` - C shim layer with `gz_deinit()`
- `embassy-nrf/src/interrupt.rs` - Interrupt binding
- `trouble-host` crate - BLE stack implementation

---

## Changelog

- 2026-03-13: Initial risk analysis document
