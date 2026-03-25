# Phase 2 Debugging Summary: Gazell Codegen Peripheral

## Overview

Goal: Get Charybdis left hand (nRF52840 nice!nano) working with `#[rmk_peripheral]` codegen + Gazell 2.4GHz -> dongle -> USB HID -> PC.

Status: **Resolved** - codegen peripheral now produces correct key output.

Final working matrix config for this hardware:
- `row2col = false`
- `low_active = true`

This issue was resolved through a combination of:
- fixing several Gazell/codegen bugs,
- adding active-LOW support for normal matrix scanning in RMK,
- and correcting the matrix direction in the Gazell split example so it matches the real electrical behavior verified by hand-written scan code.

---

## Hardware Setup

- **Peripheral**: Charybdis left hand, nice!nano v2 (nRF52840), UF2 bootloader 0.6.0 (no SoftDevice)
- **Dongle**: E104-BT5040U (nRF52840), Gazell host mode + USB HID keyboard
- **Matrix**: 5 rows x 6 cols, active-LOW scan required
- **Working electrical behavior**:
  - row_pins act as inputs with `Pull::Up`
  - col_pins act as outputs with idle HIGH
  - scan drives one output LOW
  - pressed key is detected by `is_low()` on the input pin
- **Pin config**:
  - row_pins (input side in working scan): P0_31, P1_15, P0_24, P0_22, P1_06
  - col_pins (output side in working scan): P0_02, P0_29, P0_09, P1_00, P0_11, P1_04

## Final Working Configuration

In `examples/use_config/nrf52840_gazell_split/keyboard.toml`:

```toml
[split.central.matrix]
matrix_type = "normal"
row2col = false
low_active = true

[split.peripheral.matrix]
matrix_type = "normal"
row2col = false
low_active = true
```

## Verified Working

| # | Test | Result | Notes |
|---|------|--------|-------|
| 1 | Gazell link: fake `SplitMessage::Key` -> dongle -> USB HID -> PC | **PASS** | `nrf52840_2g4` test peripheral sends fixed keys periodically |
| 2 | Codegen project diag binary (hand-written main, same Cargo.toml) | **PASS** | Proves project config, deps, memory.x, linker are correct |
| 3 | All 11 GPIO pins creation (6 Output + 5 Input) | **PASS** | Pins initialize correctly and Gazell still works |
| 4 | Manual scan: active-LOW, direct GPIO, direct `gz_send` | **PASS** | Real keys detected and sent to PC |
| 5 | Auto-send diag with no matrix | **PASS** | Proves left-hand firmware boots and Gazell path works |
| 6 | `Matrix::read_event()` -> direct `gz_send` | **PASS** | RMK matrix works when configured with the same effective electrical behavior as the hand-written scan |
| 7 | `matrix.run()` -> `KeyboardEvent` subscriber -> direct `gz_send` | **PASS** | RMK publish/subscribe path works |
| 8 | Final `#[rmk_peripheral]` binary with `row2col = false`, `low_active = true` | **PASS** | Full codegen peripheral path now works |

## Verified Failing During Investigation

| # | Test | Result | Analysis |
|---|------|--------|----------|
| 9 | Codegen `#[rmk_peripheral]` peripheral binary (original state) | **FAIL** | Multiple bugs existed at once; no useful output |
| 10 | `Matrix` before adding normal-matrix `low_active` support | **FAIL** | Wrong polarity handling caused incorrect/stuck input behavior |
| 11 | Final peripheral with `row2col = true` | **FAIL** | Even after polarity support, formal config direction still did not match the verified working scan behavior |

## Bugs Found and Fixed

### Bug 1: ISR Bridges Dropped by Proc Macro

**Files**:
- `rmk-macro/src/codegen/import.rs`
- `rmk-macro/src/codegen/split/peripheral.rs`

`expand_custom_imports()` only extracted `use` items from the user's `#[rmk_peripheral]` mod. Non-`use` items like `unsafe extern "C"` declarations and `#[interrupt]` functions were silently dropped. That removed the Gazell ISR bridges from the vector table.

**Fix**: Preserve non-`use` items from the user module and place them outside `main()` in the generated code.

### Bug 2: Default nRF52840 Config Enabled BLE for Gazell Peripheral

**File**: `rmk-config/src/communication.rs`

`default_config/nrf52840.toml` contains `[ble] enabled = true`, which could leak into Gazell peripheral codegen and cause BLE stack generation where it should not exist.

**Fix**: Early-return `CommunicationConfig::None` for Gazell split peripherals.

### Bug 3: Missing / Inconsistent Example Build Support

During debugging, the Gazell codegen example needed to be checked carefully against linker / `memory.x` / output artifact assumptions. This was a source of confusion early on, even though it was not the final blocker.

**Fix**: Rebuilt diagnostic and final artifacts directly from the example directory and verified timestamps / outputs.

### Bug 4: Double Gazell Initialization Risk

**File**: `rmk/src/split/gazell.rs`

`run_gazell_split_peripheral()` previously initialized Gazell internally. When callers also initialized Gazell before joining tasks, this created a double-init hazard.

**Fix**: Keep Gazell init at the caller boundary; `run_gazell_split_peripheral()` now assumes init has already happened.

### Bug 5: Wrong Matrix Direction in Example Config

**File**: `examples/use_config/nrf52840_gazell_split/keyboard.toml`

The final working configuration for this hardware is **not** `row2col = true`. The hand-written scan that actually worked corresponded to the generated matrix behaving as `row2col = false` in the current RMK/codegen semantics.

**Fix**: Changed both central and peripheral matrix config to `row2col = false`.

### Bug 6: Normal Matrix Had No `low_active` Support

**Files**:
- `rmk-config/src/lib.rs`
- `rmk/src/matrix.rs`
- `rmk-macro/src/codegen/chip/gpio.rs`
- `rmk-macro/src/codegen/matrix.rs`
- `rmk-macro/src/codegen/orchestrator.rs`
- `rmk-macro/src/codegen/split/peripheral.rs`

RMK already supported `low_active` for direct-pin matrix, but not for normal matrix. Charybdis left hand requires active-LOW normal-matrix scanning.

**Fix**:
- Added `MatrixConfig.low_active`
- Added `Matrix::new_with_low_active(...)`
- Updated normal matrix scanning to support active-LOW behavior
- Updated codegen so normal matrix pin initialization and construction use configured polarity

### Bug 7: Peripheral-Side Event Gating Was Too Strict

**File**: `rmk/src/split/peripheral.rs`

`SplitPeripheral` only forwarded local events when the peripheral-local mirrored `CONNECTION_STATE` was true. That could drop valid events if peripheral state lagged behind central synchronization.

**Fix**: Removed the peripheral-side send gate and always forward local events to the central.

## Final Root Cause

This was difficult because there was **not one single bug**.

The final failure required several independent conditions to overlap:

1. The Charybdis hardware requires active-LOW normal-matrix scanning.
2. RMK normal matrix originally had no `low_active` support.
3. The Gazell split example config still used the wrong direction (`row2col = true`) for the actual working behavior seen in hand-written scan tests.
4. Several earlier codegen / Gazell issues existed at the same time, which made every failed test look similar from the outside: "no key output".

That combination made the bug look like a transport issue, a matrix issue, an event-channel issue, and a macro issue at different times.

## Why It Was Hard to Debug

### 1. Many layers can fail with the same symptom

The externally visible symptom was always just:

- no key output on the PC

But that same symptom could come from:

- firmware not starting
- Gazell init / IRQ / HFCLK problems
- matrix direction mismatch
- matrix polarity mismatch
- event publish/subscribe issues
- `SplitPeripheral` dropping events
- central-side filtering

Without RTT or a debug probe, those layers all collapse into the same user-visible failure.

### 2. Hand-written diag and codegen peripheral did not share identical semantics

The hand-written diagnostics proved real hardware behavior, but they did not initially map 1:1 to the current codegen semantics of `row2col` / `col2row` / normal-matrix construction. That made some intermediate conclusions appear correct when they were only partially correct.

### 3. There were real repository bugs mixed with board-specific behavior

Some issues were **general RMK / codegen problems**:

- normal matrix had no `low_active` support
- ISR bridge handling in `#[rmk_peripheral]` was wrong
- Gazell split init ownership needed cleanup
- peripheral-side event gating was too strict

Other issues were **specific to this Charybdis hardware + config**:

- this board needs active-LOW normal-matrix scanning
- this example works with `row2col = false` in current semantics

Because both categories were present simultaneously, the debugging path was much longer than a pure project bug or a pure board-config bug would have been.

## Is This Your Unique Problem or a Repository Problem?

It is both.

### Repository-wide / generic problems

These were real upstream-quality issues in the current repository state:

- normal matrix lacking `low_active` support
- `#[rmk_peripheral]` dropping ISR bridge items
- Gazell split init boundaries needing cleanup
- peripheral-side event forwarding being too conservative

Those are not unique to your keyboard. Other boards using active-LOW normal matrices, or other Gazell peripherals depending on ISR bridge placement, could hit similar problems.

### Board-specific / configuration-specific problems

These are specific to your Charybdis left-hand setup:

- exact matrix direction / polarity behavior
- the need to align generated config with the verified hand-written scan path
- the specific pinout and diode/electrical reality of this PCB

So the short answer is:

- **No, this was not only your local mistake.**
- **Also no, it was not purely a generic RMK bug either.**
- It was a layered interaction between a real upstream gap and a hardware-specific matrix configuration.

## Key Code Files

| File | Role |
|------|------|
| `rmk/src/matrix.rs` | Matrix scanning logic and active-LOW support for normal matrix |
| `rmk-macro/src/codegen/chip/gpio.rs` | GPIO pin initialization codegen |
| `rmk-macro/src/codegen/matrix.rs` | Matrix codegen wiring |
| `rmk-macro/src/codegen/split/peripheral.rs` | Split peripheral codegen and Gazell entry |
| `rmk-macro/src/codegen/import.rs` | User mod item extraction |
| `rmk-config/src/lib.rs` | Matrix config struct (`low_active`) |
| `rmk-config/src/communication.rs` | Gazell communication override |
| `rmk/src/split/gazell.rs` | Gazell peripheral driver / entry |
| `rmk/src/split/peripheral.rs` | SplitPeripheral event loop |
| `examples/use_config/nrf52840_gazell_split/` | Codegen peripheral project |
| `examples/use_config/nrf52840_gazell_split/src/diag.rs` | Diagnostic binary used for layered isolation |
| `examples/use_rust/nrf52840_2g4/` | Manual test peripheral (working baseline) |
| `examples/use_rust/nrf52840_dongle/` | Dongle with static keymap (working baseline) |

## Final Output Artifacts

- Final working peripheral UF2: `examples/use_config/nrf52840_gazell_split/target/peripheral.uf2`
- Final diagnostic UF2: `examples/use_config/nrf52840_gazell_split/target/diag.uf2`

## Follow-up Suggestions

1. Keep `low_active` support for normal matrix as a permanent RMK feature; this is a real capability gap that affected debugging and correctness.
2. Add a small documentation note or example for active-LOW normal matrix in `keyboard.toml` examples.
3. Consider adding a regression test or expanded codegen snapshot for normal matrix with `low_active = true`.
