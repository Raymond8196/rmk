# BLE / Gazell 2.4G Hot-Switching Architecture Analysis

> Date: 2026-03-14
> Context: Phase 3 multi-pipe Gazell split complete (Steps 1-7), pending hardware verification.
> Purpose: Evaluate how current implementation affects future BLE+Gazell hot-switching and config sync.

## 1. Current Architecture

```
Compile-time mutual exclusion (split/mod.rs:22):

  compile_error!("_ble and wireless_gazell are mutually exclusive")

  BLE path:    entry.rs → ble/central.rs    (nrf-sdc/MPSL stack)
  Gazell path: entry.rs → gazell.rs         (Nordic SDK via FFI)
  Serial path: entry.rs → serial/           (UART)

  Only ONE is compiled. No runtime switching possible.
```

## 2. Radio Hardware Conflict

BLE and Gazell **cannot run simultaneously** — they share the same nRF52 RADIO peripheral.

| Resource | BLE (nrf-sdc/MPSL) | Gazell (Nordic SDK) | Conflict |
|----------|-------------------|---------------------|----------|
| RADIO IRQ | `mpsl::HighPrioInterruptHandler` | ISR bridge → `RADIO_IRQHandler` | **Same interrupt, different handler** |
| EGU0_SWI0 | `mpsl::LowPrioInterruptHandler` | ISR bridge → `SWI0_EGU0_IRQHandler` | **Same interrupt** |
| TIMER0 | MPSL managed | Not used | No conflict |
| TIMER2 | Not used | ISR bridge → `TIMER2_IRQHandler` | No conflict |

**Conclusion**: Time-division multiplexing is feasible — stop one protocol, release radio, start the other. Estimated switch latency: 50-200ms.

## 3. What Current Design Gets Right

These design choices are **favorable** for future hot-switching:

| Design | Location | Benefit |
|--------|----------|---------|
| `SplitReader` / `SplitWriter` traits | `driver.rs:30-37` | `PeripheralManager` is transport-agnostic |
| `PeripheralManager<T>` generic | `driver.rs:46-57` | Works with any T that implements Reader+Writer |
| Shared `SplitMessage` enum | `mod.rs:35-58` | BLE and Gazell use identical message format |
| `PipeDriver` channel decoupling | `gazell.rs` | Hub ↔ manager via channels; stopping hub preserves manager state |
| `DummyMatrix` for zero-matrix central | `matrix.rs` | Dongle architecture is transport-independent |

## 4. What Blocks Hot-Switching

| Blocker | Location | Issue | Required Change |
|---------|----------|-------|-----------------|
| `compile_error!` guard | `split/mod.rs:22-26` | BLE + Gazell cannot coexist in one binary | Remove; use runtime `AtomicBool` mutual exclusion |
| `#[cfg]` on function signatures | `central.rs:29-39` | BLE args vs Gazell args selected at compile time | Unify signature or use enum wrapper |
| if-else codegen chain | `entry.rs:103/163` | Only one connection type emitted | Emit both; runtime activation |
| Interrupt binding | `bind_interrupt.rs:87-131` vs `228-261` | Same IRQ → different handler, compile-time | Indirect jump table or runtime rebind |
| Hub is infinite (`!`) | `run_gazell_central_hub` | Cannot stop gracefully | Add cancellation token / shutdown signal |
| `gz_init_default(1)` in codegen | `split/central.rs` | Gazell inits at startup unconditionally | Defer to runtime / conditional init |

## 5. SplitMessage — Config Sync Gap

Current variants (9 total):

```
Peripheral → Central:  Key, Touchpad, Pointing, BatteryState
Central → Peripheral:  LedState, ConnectionState, KeyboardIndicator, Layer
BLE-only (not gated):  Address, ClearPeer
```

Missing for config sync:

| Variant | Direction | Purpose |
|---------|-----------|---------|
| `KeymapSync(layer, row, col, action)` | Central → Peripheral | Propagate Vial keymap changes |
| `TransportSwitch(mode)` | Central → Peripheral | Coordinate BLE/Gazell mode switch |
| `ConfigQuery` / `ConfigResponse` | Bidirectional | Re-sync after reconnection |

**Not a blocker** — `SplitMessage` is an enum, adding variants is additive. Current design does not prevent future extension.

## 6. Storage Feature Gate

```rust
// driver.rs:7-8 — only available when BOTH storage AND _ble are enabled
#[cfg(all(feature = "storage", feature = "_ble"))]
use {FLASH_CHANNEL, PeerAddress, FlashOperationMessage};
```

Gazell dongle currently has `storage.enabled = false`. To support Vial keymap persistence on dongle, the gate needs relaxation from `storage + _ble` to `storage` alone.

## 7. Feasible Switching Architectures

### Option A: Time-Division Radio Multiplexing (Recommended)

```
                  ┌── Gazell Mode ──┐
Peripheral ──────→│  RADIO/TIMER2   │──────→ Dongle (USB HID → PC)
                  │  (2.4GHz, fast) │
                  └─────────────────┘
                       ↕ switch (~100ms)
                  ┌─── BLE Mode ────┐
Peripheral ──────→│  RADIO/MPSL     │──────→ Phone / Tablet
                  │  (BLE, standard)│
                  └─────────────────┘
```

- Stop current stack → release radio → init new stack
- Current `SplitReader/Writer` + `PeripheralManager` directly reusable
- Main work: codegen emits dual-mode init, runtime switch logic

### Option B: Dual Binary + Bootloader

- Two firmware images, DFU to switch
- Simple but slow (seconds), poor UX

### Option C: MPSL Timeslot API

- Nordic MPSL supports sharing radio between BLE and proprietary protocols
- Requires rewriting Gazell as MPSL timeslot client
- Extremely high effort, not recommended

## 8. Preparatory Changes (Optional, Pre-HW-Verification)

Changes that could be done proactively to reduce future hot-switching effort:

| Change | Priority | Effort | Status |
|--------|----------|--------|--------|
| Add cancel token to hub + peripheral + pipe manager | Medium | Small | ✅ Done (poison pill via `Option<SplitMessage>`) |
| Relax storage gate to `#[cfg(feature = "storage")]` | Medium | Small | ✅ Not needed (already works) |
| Remove `compile_error!` → runtime guard | Low | **Medium** | ⬜ Requires signature refactor (see below) |
| Unify `central.rs` / `peripheral.rs` function signature | Low | Medium | ⬜ Must be done together with compile_error! removal |
| Emit dual-mode codegen in `entry.rs` | Low | Large | ⬜ Core hot-switching infrastructure |

**Note on `compile_error!` removal** (2026-03-25): Cannot simply remove the guard. `peripheral.rs:37-67` and `central.rs:25-58` use `#[cfg(feature = "_ble")]` and `#[cfg(feature = "wireless_gazell")]` on individual generic parameters and function arguments. When both features are enabled, all params exist and both code paths execute sequentially — not the intended mutual exclusion. Requires refactoring to enum-based dispatch.

## 9. Summary

**Runtime abstraction layer** (traits, PeripheralManager, SplitMessage, channels): **friendly** to hot-switching. Transport is pluggable.

**Main obstacle**: Codegen-level compile-time hardcoding of interrupt bindings and radio initialization. Needs "dual-mode init + runtime switch" codegen path instead of "compile-time pick one."

**Config sync**: No architectural blocker. Add SplitMessage variants + relax storage feature gate.
