# Phase 3: Multi-Pipe Gazell + Codegen + keyboard.toml Integration

> **Branch**: `feat/gazell-2g4-verify`
> **Status**: Planning complete, implementation pending
> **Prerequisite**: Phase 2 single-pipe Gazell bidirectional communication (done)

## 1. Overview

Phase 2 established single-pipe Gazell bidirectional communication with `GazellCentralDriver` and `GazellPeripheralDriver` (implemented in `rmk/src/split/gazell.rs`). Phase 3 extends this to a production-ready split keyboard system where:

- Both Charybdis halves connect to a USB dongle via Gazell 2.4GHz
- The dongle runs full RMK keyboard processing (keymap/layers/macros)
- The dongle outputs USB HID to the host PC
- Configuration is driven by `keyboard.toml` with codegen support

### Target Architecture

```
Left Hand (nRF52840, peripheral)
    |  Gazell pipe 0
    v
USB Dongle (nRF52840, central)  ──USB HID──>  PC
    ^
    |  Gazell pipe 1
Right Hand (nRF52840, peripheral)
```

## 2. Lessons from Phase 2

| Lesson | Application in Phase 3 |
|---|---|
| FFI type mismatch (uint8_t vs uint32_t) caused stack corruption | Multi-pipe hub reuses existing verified FFI; no new C shim changes |
| Mock fallbacks critical for host testing | All new static channels and hub logic must work in mock mode (non-ARM) |
| Build examples from their own directories | Verification commands always `cd` into example dir first |
| State vs Event distinction is critical | Hub's flush logic preserves P2's `pending_event > pending_state` priority per-pipe |
| `compile_error!` guard prevents BLE+Gazell co-existence | Maintained; codegen emits `wireless_gazell_nrf52840` feature, not `_ble` |
| Each step must be independently verifiable | Every step has explicit verification commands and success criteria |

## 3. Self-Review: Critical Issues Identified

During plan review, 6 blocker-level issues were found by tracing actual source code:

| # | Issue | Impact | Fix (Step) |
|---|---|---|---|
| 1 | `bind_interrupt_default()` calls `communication.get_ble_config().unwrap()` at `bind_interrupt.rs:100` — panics when Gazell (no `[ble]` in TOML) | **Blocker**: central won't compile | Step 5: short-circuit nRF52 path for Gazell BEFORE ble_config access |
| 2 | `expand_bind_interrupt_for_split_peripheral()` calls `get_ble_config().unwrap()` at `peripheral.rs:77` | **Blocker**: peripheral won't compile | Step 5: add Gazell path in peripheral ISR codegen |
| 3 | `expand_matrix_config()` at `matrix.rs:60` calls `row_pins.clone().unwrap()` — panics when rows=0 cols=0 | **Blocker**: zero-matrix panics | Step 4: guard `expand_matrix_config` AND `expand_matrix_and_keyboard_init` |
| 4 | `rmk_entry_select()` at `entry.rs:55` always adds `matrix` to devices — but zero-matrix has no `matrix` var | **Blocker**: undefined variable | Step 3+4: conditionally skip `matrix` in devices |
| 5 | Original steps 7 and 8 were duplicate | Plan clarity | Merged into single Step 7 |
| 6 | `peripheral.rs` BatteryState uses `with_feature("_ble")` macro, not `#[cfg]` | Step 6 scope | Step 6: also handle `select_biased_with_feature!` macro invocations |

## 4. Steps

### Step 0: Merge upstream/main

**Status**: DONE

All main refactors (PR #726 `refactor/macro`, PR #717 `feat/event`) already merged into our branch.

---

### Step 1: Multi-Pipe Demultiplexer (GazellCentralHub)

**Problem**: Current `GazellCentralDriver` (`rmk/src/split/gazell.rs:168-337`) calls `gz_recv()` per-instance. With two peripherals, two driver instances would race for the same hardware FIFO — packets get stolen by the wrong driver.

**Solution**: Single `GazellCentralHub` async task owns `gz_recv()`, dispatches to per-pipe embassy Channels. Per-pipe `PipeDriver` implements `SplitReader + SplitWriter` via channel send/recv.

**Architecture**:
```
                gz_recv()
                   |
             GazellCentralHub  (single task)
              /            \
    PIPE_RX[0]             PIPE_RX[1]        (embassy Channel, capacity 8)
         |                      |
   PipeDriver(0)   PipeDriver(1)
         |                      |
   PeripheralManager(L)   PeripheralManager(R)
```

**Files**:
- `rmk/src/split/gazell.rs` (modify existing)
- `rmk/src/split/central.rs` (update caller)

**Key Changes**:

1. **Static channel arrays** (MAX_GAZELL_PIPES = 8, Gazell hardware max). Runtime `num_pipes` controls how many are used:
   ```rust
   pub(crate) const MAX_GAZELL_PIPES: usize = 8;
   static PIPE_RX: [Channel<RawMutex, SplitMessage, 8>; MAX_GAZELL_PIPES] = [
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
   ];
   static PIPE_TX: [Channel<RawMutex, SplitMessage, 4>; MAX_GAZELL_PIPES] = [
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
   ];
   ```

2. **GazellCentralHub** async fn: owns `gz_recv()` loop, dispatches by `rx_pipe` to `PIPE_RX[rx_pipe]`, filters heartbeats, flushes `PIPE_TX[i]` for all active pipes via `gz_set_ack_payload(pipe_i, ...)`. Flush priority: event > state (reuse `is_event_type()`).

3. **PipeDriver** `{ pipe_index: usize }` — SplitReader/SplitWriter via channel send/recv. **No Gazell-specific code** — fully reusable for other radio protocols (ESB, ESP-NOW).

4. **run_gazell_central_hub(config, num_pipes)** — init Gazell host mode, hub loop wrapped in `select(hub_loop, GAZELL_SHUTDOWN.wait())` for Phase 4 hot-switch readiness.

5. **run_gazell_pipe_manager\<ROW, COL, ROW_OFFSET, COL_OFFSET\>(pipe_index, id)** — creates PipeDriver + PeripheralManager, runs.

6. Keep `GazellPeripheralDriver` unchanged (keyboard half still uses direct FFI).

7. Migrate `run_gazell_peripheral_manager` in `central.rs:43-44` to call hub-based pipe manager instead.

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"` | Host compile with Gazell features |
| V2 | `cargo test --manifest-path rmk/Cargo.toml --lib -- gazell` | Unit tests in mock mode |
| V3 | `cargo check --manifest-path rmk/Cargo.toml --features "split"` | Serial split regression |
| V4 | `cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840` | ARM FFI cross-compile |
| V5 | `cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -` | Existing example still builds |

**Success Criteria**: All 5 commands pass. New types (`GazellCentralHub`, `PipeDriver`) visible in `cargo doc`.

---

### Step 2: rmk-config — Gazell Split Fields

**Problem**: `SplitConfig` (`rmk-config/src/lib.rs:775-782`) has no Gazell-specific fields.

**Files**: `rmk-config/src/lib.rs`

**Changes**:
1. Add `GazellSplitConfig` struct with `channel`, `data_rate`, `tx_power`, `heartbeat_interval_ms` (all `#[serde(default)]`)
2. Add `gazell: Option<GazellSplitConfig>` to `SplitConfig`
3. Add `gazell_pipe: Option<u8>` to `SplitBoardConfig`
4. Add `#[serde(default)]` to `SplitBoardConfig.matrix` field (line 804) — allows omitting `[split.central.matrix]` for dongle

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-config/Cargo.toml` | Config crate compiles |
| V2 | `cargo test --manifest-path rmk-config/Cargo.toml` | Config tests pass, existing TOML parsing not broken |

**Success Criteria**: Both pass. New structs appear in `cargo doc`.

---

### Step 3: Codegen — "gazell" Connection Type

**Problem**: Codegen panics at `entry.rs:164` for unknown connection types. The codegen dispatch chain (`entry.rs`, `split/central.rs`, `split/peripheral.rs`) has no `"gazell"` path.

**Files**:
- `rmk-macro/src/codegen/entry.rs` — add `"gazell"` branch
- `rmk-macro/src/codegen/split/central.rs` — add `"gazell"` arm in `expand_split_communication_config()`
- `rmk-macro/src/codegen/split/peripheral.rs` — add `"gazell"` branch in `expand_split_peripheral_entry()`

**Changes**:

**3a. entry.rs** — central dispatch (after `"serial"` block, before panic):
```rust
} else if split_config.connection == "gazell" {
    let rmk_task = quote! { ::rmk::run_rmk(#keymap #usb_driver_arg #storage rmk_config) };
    let num_peripherals = split_config.peripheral.len();
    tasks.push(rmk_task);
    // Hub task
    tasks.push(quote! { ::rmk::split::gazell::run_gazell_central_hub(gazell_config, #num_peripherals) });
    // Per-peripheral pipe manager tasks
    for (idx, p) in split_config.peripheral.iter().enumerate() { ... }
    join_all_tasks(tasks)
}
```

**Critical guard** (addresses blocker #4): When `central.rows == 0 && central.cols == 0`, do NOT push `matrix` to `devs`:
```rust
let devices_task = if is_zero_matrix_central { /* skip matrix */ } else { /* existing code */ };
```

**3b. split/central.rs** — `"gazell"` match arm generates `GazellConfig` from TOML fields.

**3c. split/peripheral.rs** — `"gazell"` branch generates `GazellConfig` with peripheral's pipe, calls `run_rmk_split_peripheral(gazell_config)`.

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | Macro crate compiles |

**Success Criteria**: Macro crate compiles. Full integration deferred to Step 7.

---

### Step 4: Zero-Matrix Central (Dongle Has No Keys)

**Problem**: Multiple codegen functions unconditionally generate matrix code, causing panics when rows=0, cols=0 (dongle has no key matrix).

**Panic Points**:
1. `expand_matrix_config()` at `matrix.rs:60` — `row_pins.clone().unwrap()` panics
2. `expand_matrix_and_keyboard_init()` at `orchestrator.rs:338-363` — generates `Matrix::new()` with missing pins
3. `rmk_entry_select()` at `entry.rs:55` — adds `matrix` variable to devices task, but variable doesn't exist

**Files**:
- `rmk-macro/src/codegen/matrix.rs` — guard for zero-matrix
- `rmk-macro/src/codegen/orchestrator.rs` — guard for zero-matrix
- `rmk-macro/src/codegen/entry.rs` — conditionally skip `matrix` in devices (co-addressed with Step 3)

**Changes** (when `split.central.rows == 0 && split.central.cols == 0`):
1. `expand_matrix_config()` returns `quote! {}` (empty)
2. `expand_matrix_and_keyboard_init()` returns only `Keyboard::new(&keymap)`, no matrix
3. Entry task list has no `matrix` in devices, no matrix scanning task
4. Keyboard task still runs (event-driven, receives from PeripheralManagers via channel)

**Feasibility (verified)**:
- `Keyboard::new(&keymap)` at `keyboard.rs:256` — takes only `&RefCell<KeyMap>`, no matrix dependency
- `Keyboard::run()` at `keyboard.rs:156-175` — event-driven via `keyboard_event_subscriber.receive()`
- `timer: [[None; 0]; 0]` is a valid zero-sized array (ZST)

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | Macro crate compiles with zero-matrix guard |
| V2 | Step 7 example build | Full integration test |

**Success Criteria**: No matrix code generated for zero-matrix central. Keyboard task still generated.

---

### Step 5: Gazell ISR Bridge Codegen

**Problem**: Gazell needs ISR bridges (RADIO, TIMER2, EGU0_SWI0) instead of BLE nrf-sdc. Current codegen always emits BLE code for nRF52, and **panics** when `[ble]` section is missing from TOML (blockers #1, #2).

**Critical Panic Points**:
- `bind_interrupt_default()` at `bind_interrupt.rs:100` — `communication.get_ble_config().unwrap()` with no `[ble]`
- `expand_bind_interrupt_for_split_peripheral()` at `peripheral.rs:77` — same issue

**Files**:
- `rmk-macro/src/codegen/chip/bind_interrupt.rs` — add Gazell path in `bind_interrupt_default()`
- `rmk-macro/src/codegen/split/peripheral.rs` — add Gazell path in `expand_bind_interrupt_for_split_peripheral()`

**Changes for central** (`bind_interrupt_default`, nRF52 path):

Check `split_config.connection == "gazell"` BEFORE accessing ble_config. When Gazell:

1. Generate ISR bridges (not `bind_interrupts!`):
   ```rust
   extern "C" { fn RADIO_IRQHandler(); fn TIMER2_IRQHandler(); fn SWI0_EGU0_IRQHandler(); }
   #[pac::interrupt] fn RADIO() { unsafe { RADIO_IRQHandler() } }
   #[pac::interrupt] fn TIMER2() { unsafe { TIMER2_IRQHandler() } }
   #[pac::interrupt] fn EGU0_SWI0() { unsafe { SWI0_EGU0_IRQHandler() } }
   ```
2. Generate interrupt priority setup (not `mpsl_task`/`build_sdc`):
   ```rust
   interrupt::RADIO.set_priority(Priority::P0);
   interrupt::TIMER2.set_priority(Priority::P0);
   interrupt::EGU0_SWI0.set_priority(Priority::P1);
   ```
3. Keep USB interrupt binding (dongle uses USB HID)
4. No `nrf_sdc` dependency — Gazell doesn't use Softdevice Controller

**Changes for peripheral** (`expand_bind_interrupt_for_split_peripheral`, nRF52 path):

Same pattern — check connection type first, emit Gazell ISR bridges instead of BLE nrf-sdc code.

**Detection method**: Extract from `BoardConfig::Split(split_config)` -> `split_config.connection`. The `board` variable is already available in both functions.

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | Macro crate compiles |
| V2 | Step 7 example build | No `nrf_sdc` code generated for Gazell |

**Success Criteria**: Macro crate compiles. No `nrf_sdc` code path triggered for Gazell.

---

### Step 6: BatteryState Feature Gate Fix

**Problem**: `SplitMessage::BatteryState` is gated behind `#[cfg(feature = "_ble")]` only. Gazell peripherals have batteries too and need to report battery state.

**All Affected Locations**:
- `rmk/src/split/mod.rs:4-5` — `use crate::event::BatteryStateEvent` import
- `rmk/src/split/mod.rs:56-57` — `BatteryState(BatteryStateEvent)` variant
- `rmk/src/split/driver.rs:15-16` — `PeripheralBatteryEvent` import
- `rmk/src/split/driver.rs:243-247` — `SplitMessage::BatteryState` match arm
- `rmk/src/split/peripheral.rs:12-16` — imports (`BatteryStateEvent`, `ChargingStateEvent`, etc.)
- `rmk/src/split/peripheral.rs:88-93` — subscriber creation (`charging_state_sub`, `battery_sub`)
- `rmk/src/split/peripheral.rs:99-108` — `with_feature("_ble")` in `select_biased_with_feature!` macro

**Changes**: Replace `#[cfg(feature = "_ble")]` with `#[cfg(any(feature = "_ble", feature = "wireless_gazell"))]` at all locations. For `select_biased_with_feature!` macro invocations, replace `with_feature("_ble")` with the appropriate dual-feature gate (check macro support, fallback to `cfg_attr` if needed).

**Verification Plan**:

| # | Command | What It Tests |
|---|---|---|
| V1 | `cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"` | BatteryState available under Gazell |
| V2 | `cargo check --manifest-path rmk/Cargo.toml --features "split,_nrf_ble"` | BLE regression |
| V3 | `cargo check --manifest-path rmk/Cargo.toml --features "split"` | Serial split (no wireless) |
| V4 | `cargo test --manifest-path rmk/Cargo.toml --lib -- split` | Unit tests |

**Success Criteria**: All 4 commands pass.

---

### Step 7: Gazell Split Example (Integration Test)

**Problem**: Need an end-to-end example that exercises all previous steps together (codegen, config, ISR bridges, zero-matrix, multi-pipe).

**Files**: New `examples/use_config/nrf52840_gazell_split/` following BLE split example pattern.

**Structure** (mirrors `nrf52840_ble_split/`):
```
nrf52840_gazell_split/
+-- keyboard.toml
+-- Cargo.toml           # rmk with wireless_gazell_nrf52840 feature
+-- .cargo/config.toml
+-- memory.x
+-- src/
    +-- central.rs       # #[rmk_central] mod keyboard_central {}
    +-- peripheral.rs    # #[rmk_peripheral(id = 0)] mod keyboard_peripheral {}
```

**keyboard.toml** key points:
- `chip = "nrf52840"`, `usb_enable = true` (dongle has USB)
- `connection = "gazell"`
- `[split.central]` rows=0, cols=0 (no keys on dongle)
- No `[split.central.matrix]` section (omitted, uses Default via `#[serde(default)]`)
- Two `[[split.peripheral]]` with `gazell_pipe = 0` / `gazell_pipe = 1`
- No `[ble]` section (Gazell only)

**Cargo.toml** key deps:
- `rmk` with `wireless_gazell_nrf52840` feature (includes `split`)
- `rmk-gazell-sys` with `nrf52840` feature
- `embassy-nrf`, `cortex-m`, `defmt` (no `nrf-sdc`, no `bt-hci`)

**Verification Plan** (integration test for Steps 1-6):

| # | Command | What It Tests |
|---|---|---|
| V1 | `cd examples/use_config/nrf52840_gazell_split && cargo build --release --bin central && cd -` | Central ARM build (exercises Steps 1,3,4,5) |
| V2 | `cd examples/use_config/nrf52840_gazell_split && cargo build --release --bin peripheral && cd -` | Peripheral ARM build (exercises Steps 3,5,6) |

**Success Criteria**: Both ARM builds succeed, producing valid ELF binaries.

---

### Step 8: Hardware Verification

**Problem**: Software builds don't guarantee RF communication works end-to-end.

**Test Matrix**:

| Test | Action | Expected Result |
|------|--------|-----------------|
| V1 | Press 'A' on left hand | 'A' appears on PC |
| V2 | Press 'L' on right hand | 'L' appears on PC |
| V3 | Both hands simultaneously | Both keys register |
| V4 | Hold layer key on left, press on right | Layer-shifted action |
| V5 | Toggle CapsLock from PC | LED on both peripherals |
| V6 | Unplug dongle USB | Both peripherals detect disconnect |

**Success Criteria**: All 6 tests pass.

---

## 5. Dependency Graph

```
Step 0 (merge upstream) -- DONE
 |-- Step 1 (multi-pipe demux)        <-- runtime, independent
 |-- Step 2 (rmk-config)              <-- config, independent
 +-- Step 6 (battery gate fix)        <-- runtime, independent
       |
Step 3 (codegen) <-- depends on 1, 2
Step 4 (zero-matrix) <-- depends on 3
Step 5 (ISR codegen) <-- depends on 3
       |
Step 7 (example) <-- depends on 3, 4, 5, 6  [INTEGRATION TEST]
       |
Step 8 (hardware verification) <-- depends on 7
```

**Parallelizable**: Steps 1, 2, 6 are independent. Steps 4, 5 are independent (after 3).

## 6. Key Source References

| Claim | Source |
|---|---|
| GazellCentralDriver does per-instance `gz_recv()` | `rmk/src/split/gazell.rs:237-290` |
| SplitReader/SplitWriter traits | `rmk/src/split/driver.rs:30-37` |
| PeripheralManager generic over transport | `rmk/src/split/driver.rs:46-57` |
| Embassy Channel static pattern | `rmk/src/channel.rs:17` |
| BatteryState gated behind `_ble` only | `rmk/src/split/mod.rs:4-5,56-57` |
| Codegen entry dispatch on connection string | `rmk-macro/src/codegen/entry.rs:103,129,164` |
| Central codegen communication config | `rmk-macro/src/codegen/split/central.rs:17-38` |
| ISR bridge codegen for BLE central (nRF52) | `rmk-macro/src/codegen/chip/bind_interrupt.rs:87-214` |
| `get_ble_config().unwrap()` panics without `[ble]` | `bind_interrupt.rs:100`, `peripheral.rs:77` |
| Matrix pin codegen `.unwrap()` panics | `rmk-macro/src/codegen/matrix.rs:60` |
| `Keyboard::new()` takes only `&keymap` | `rmk/src/keyboard.rs:256` |
| `Keyboard::run()` event-driven | `rmk/src/keyboard.rs:156-175` |
| `run_rmk()` USB-only path | `rmk/src/lib.rs:272` |
| `wireless_gazell_nrf52840` feature includes `split` | `rmk/Cargo.toml:231` |

## 7. Phase 4 Readiness & ESB Portability

### Architecture Layering

```
[Chip-specific -- must rewrite for ESB/ESP-NOW]
  rmk-gazell-sys              FFI crate
  GazellPeripheralDriver      direct FFI calls (gz_send, gz_get_ack_payload)
  GazellCentralHub (P3)       direct FFI calls (gz_recv, gz_set_ack_payload)

[Protocol-agnostic -- reusable for ANY multi-pipe radio]
  PipeDriver (P3)             channel-based SplitReader+SplitWriter (no FFI)
  static PIPE_RX/PIPE_TX      embassy Channel arrays

[Transport-agnostic -- fully reusable]
  SplitReader/SplitWriter     traits (rmk/src/split/driver.rs:30-37)
  PeripheralManager           generic over T: SplitReader+SplitWriter
  SplitPeripheral             generic over S: SplitWriter+SplitReader
  Keyboard / event system     no transport coupling
```

### Hot-Switch Readiness

Hub uses `select(hub_loop, GAZELL_SHUTDOWN.wait())` — Phase 3 never triggers the signal, Phase 4 triggers it to switch radio mode (BLE <-> Gazell).

### ESB / Other-Chip Portability

Rewrite boundary is clean: FFI + PeripheralDriver + CentralHub (bottom layer). Everything above PipeDriver reuses unchanged.

## 8. Resolved Questions

| Question | Answer | Source |
|---|---|---|
| Step 0 merge status | Already merged | `git merge-base --is-ancestor main HEAD` |
| MAX_GAZELL_PIPES | 8 (hardware max), runtime `num_pipes` param | User requirement |
| Old use_rust examples | Migrate to hub architecture | User requirement |
| Zero-matrix feasibility | Keyboard is event-driven, no matrix dependency | `keyboard.rs:156,256` |
| TOML without `[split.central.matrix]` | Add `#[serde(default)]` | `rmk-config/src/lib.rs:804` |
| Dongle USB (no BLE) | `run_rmk()` USB-only path works | `rmk/src/lib.rs:272` |
| `get_ble_config().unwrap()` panic | Must short-circuit for Gazell before BLE code | `bind_interrupt.rs:100`, `peripheral.rs:77` |
