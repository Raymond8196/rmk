# RMK Firmware Development Guide

> This document defines Claude Code working standards for the RMK keyboard firmware project.
> Update this file whenever Claude makes mistakes or when new standards emerge - forming a flywheel effect (Boris Cherny Tip #4).

English Version | [中文版本](./CLAUDE.zh.md)

## Language Policy

**IMPORTANT**:
- All documentation, commit messages, code comments, and PR descriptions **MUST be in English**
- Conversation with the user can be in Chinese or English
- This ensures the project is accessible to the international community

## Project Overview

### RMK (Rust Mechanical Keyboard)
- **Language**: Rust (no_std)
- **Framework**: Embassy async
- **Supported MCUs**: STM32, nRF52, RP2040, ESP32
- **Core Features**: Keyboard firmware, split keyboard, BLE/USB communication, pointing devices

### Current Work: `feat/gazell-2g4` Branch
- **Goal**: Nordic Gazell 2.4GHz wireless protocol support (keyboard <-> USB dongle)
- **Hardware**: Charybdis split keyboard (nRF52840) + E104-BT5040U dongle (nRF52840)
- **Architecture**:
  ```
  Keyboard (nRF52840, device mode)
      ↓  Gazell 2.4GHz
  Dongle (E104-BT5040U, host mode)
      ↓  USB HID
  PC
  ```
- **Key Files**:
  - `rmk-gazell-sys/` — FFI crate: C shim + Nordic SDK bindings
  - `rmk/src/wireless/gazell.rs` — `GazellTransport` safe wrapper
  - `rmk/src/wireless/config.rs` — `GazellConfig` configuration
  - `rmk/src/wireless/transport.rs` — `WirelessTransport` trait
  - `rmk/src/split/gazell.rs` — `GazellCentralHub`, `PipeDriver`, split drivers
  - `examples/use_config/nrf52840_gazell_split/` — Charybdis codegen example (central + 2 peripherals)
  - `examples/use_rust/nrf52840_dongle/` — Host/receiver standalone example (Phase 1/2)
  - `examples/use_rust/nrf52840_radio_switch_poc/` — Phase 4.1 PoC: dynamic RADIO dispatch (Gazell + BLE coexist)
  - `examples/use_rust/nrf52840_2g4/` — Device/transmitter standalone example (Phase 1)

### Branch Progress Summary

**Phase 1: Minimal TX/RX verification**
| Feature | Status |
|---------|--------|
| `rmk-gazell-sys` FFI crate (C shim + build.rs) | ✅ Done |
| `GazellTransport` implementing `WirelessTransport` trait | ✅ Done |
| Mock implementation for testing without hardware | ✅ Done |
| Dongle example (host mode, USB CDC debug) | ✅ Done |
| Keyboard example (device mode, test packets) | ✅ Done |
| Nordic nRF5 SDK v17.1.0 setup | ✅ Installed |
| Cross-compile verification (ARM target) | ✅ Both ~25KB |
| Hardware validation (dongle <-> keyboard) | ✅ Done (2.4GHz communication verified) |

**Phase 2: SplitMessage over Gazell + Charybdis integration**
| Feature | Status |
|---------|--------|
| C shim `ack_payload_length` type fix (Step 1) | ✅ Done |
| Rust FFI bindings: `gz_set_ack_payload` / `gz_get_ack_payload` (Step 2) | ✅ Done |
| `GazellConfig` add `heartbeat_interval_ms`, update call sites (Step 3) | ✅ Done |
| `GazellSplitDriver` (Peripheral + Central) (Step 4) | ✅ Done |
| Wire into split module + `compile_error!` guard (Step 5) | ✅ Done |
| Cargo.toml feature gate update (Step 6) | ✅ Done |
| Codegen: `rmk-macro` add `"gazell"` connection type (Step 9) | ✅ Done |
| `rmk-config` override CommunicationConfig for Gazell (Step 10) | ✅ Done |
| Dongle example: USB HID forwarding with static keymap (Step 11) | ✅ Done + HW verified |
| Test peripheral: SplitMessage::Key via gz_send (Step 11b) | ✅ Done + HW verified |
| Charybdis keyboard.toml codegen peripheral (Step 12) | ✅ Done + HW verified |
| Hardware verification: left hand keypress → dongle → PC (Step 13) | ✅ Done (row2col=true, build.rs fix) |

**Phase 3: Multi-pipe Gazell split (codegen-driven central)**
| Feature | Status |
|---------|--------|
| `GazellCentralHub` + `PipeDriver` (multi-pipe demux via channels) | ✅ Done |
| `rmk-config`: `GazellSplitConfig`, `gazell_pipe` field, `#[serde(default)]` on matrix | ✅ Done |
| `get_communication_config()` fix: Gazell central → `Usb(...)`, peripheral → `None` | ✅ Done |
| Zero-matrix central: `DummyMatrix`, skip pin init for rows=0/cols=0 | ✅ Done |
| Codegen: hub + pipe manager task spawning in `entry.rs` | ✅ Done |
| ISR bridge codegen for both central and peripheral (bind_interrupt.rs, peripheral.rs) | ✅ Done |
| Central codegen: HFCLK + IRQ priority + `gz_init_default(1)` | ✅ Done |
| `_wireless` feature gate for `BatteryState` | ✅ Done |
| Example: `nrf52840_gazell_split` — central + peripheral + peripheral_right (ARM builds) | ✅ Done |
| Hardware verification: dual-hand keypress → dongle → PC | ✅ Done |
| PMW3610 trackball over Gazell (peripheral → central → USB HID mouse) | ✅ Done |
| `FlexPin` feature gate: `wireless_gazell` + `dep:embassy-nrf` for bitbang SPI | ✅ Done |
| Startup grace period (2s) for PMW3610 init before Gazell heartbeat | ✅ Done |
| Peripheral disconnect timeout (1s) with automatic key release | ✅ Done |

**Phase 4: BLE ↔ Gazell hot-switching**
| Feature | Status |
|---------|--------|
| Architecture analysis (`ble_gazell_switching_analysis.md`) | ✅ Done |
| Risk analysis (`ble_gazell_switching_risk_analysis.md`) | ✅ Done |
| Cancel tokens: `GAZELL_HUB_CANCEL`, `GAZELL_PERIPHERAL_CANCEL` | ✅ Done |
| Poison pill for PipeDriver (`PIPE_RX` → `Option<SplitMessage>`) | ✅ Done |
| Storage gate analysis (already works, no change needed) | ✅ Done |
| Remove `compile_error!`, split entry functions per transport | ✅ Done |
| Dynamic RADIO interrupt dispatcher | ✅ Done + HW verified |
| BLE stack encapsulation (trouble-host pause/restart) | ⬜ Phase 4.2 |
| Unified ConnectionManager + keycode switching | ⬜ Phase 4.3 |

**Architecture decisions**:
- Left hand (peripheral) → Gazell → dongle → USB HID → PC (Phase 2, verified)
- Both hands → Gazell → dongle → USB HID → PC (Phase 3, verified)
- Dongle/central: uses `keyboard.toml` codegen (`#[rmk_central]`), zero-matrix with `rows=0/cols=0`
- `examples/use_rust/nrf52840_dongle/` retained as standalone reference (Phase 1/2 demo)
- Trackball (pmw3610) data: over Gazell, verified — peripheral runs sensor + sends PointingEvent via SplitMessage
- Cancel token design: `Signal<RawMutex, ()>` with `signaled()` (non-consuming read); poison pill via `Option<SplitMessage>` channel for PipeDriver
- `compile_error!` removed — `peripheral.rs` / `central.rs` refactored from one unified function with `#[cfg]` on generic params into separate transport-specific entry functions (`_ble`, `_gazell`, `_serial`), allowing `_ble` + `wireless_gazell` to coexist
- Dynamic RADIO dispatch: `AtomicU8` selector in ISR (0=idle, 1=Gazell, 2=BLE); manual `unsafe impl Binding` for MPSL to bypass `bind_interrupts!`; TIMER2 (Gazell-only) and TIMER0/RTC0 (MPSL-only) are always bridged (no conflict); only RADIO and EGU0_SWI0 need dynamic dispatch; `nrf-mpsl` critical-section-impl replaces cortex-m single-core (link conflict)

### Environment Setup
```bash
# Nordic SDK (required for ARM cross-compilation)
export NRF5_SDK_PATH="$HOME/nRF5_SDK_17.1.0/nRF5_SDK_17.1.0_ddde560"

# ARM toolchain
sudo apt-get install -y gcc-arm-none-eabi

# Rust target
rustup target add thumbv7em-none-eabihf

# Build commands
# IMPORTANT: Examples MUST be built from their own directories (not repo root)
# because cargo resolves .cargo/config.toml from CWD, not --manifest-path.
# Building from repo root with --manifest-path will miss linker scripts (-Tlink.x)
# and produce empty ELF binaries.
cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840
cd examples/use_rust/nrf52840_dongle && cargo build --release
cd examples/use_rust/nrf52840_2g4 && cargo build --release
```

### Firmware Generation & Flashing

All commands run from `examples/use_config/nrf52840_gazell_split/`.

```bash
# 1. Build all three binaries
cargo build --release

# 2. Keyboards (nice!nano, UF2 bootloader, no SoftDevice → base 0x1000)
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/peripheral target/peripheral.bin
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/peripheral_right target/peripheral_right.bin
python bin2uf2.py target/peripheral.bin target/peripheral.uf2 0x1000
python bin2uf2.py target/peripheral_right.bin target/peripheral_right.uf2 0x1000

# 3. Dongle (E104-BT5040U, nRF52840 DFU bootloader)
arm-none-eabi-objcopy -O ihex target/thumbv7em-none-eabihf/release/central target/central.hex
python -m nordicsemi pkg generate --hw-version 52 --sd-req 0x00 \
    --application target/central.hex --application-version 1 \
    target/central_dfu.zip
```

**Flashing**:
- **Keyboards**: Double-click reset → enter UF2 mode → drag `.uf2` onto the USB drive
- **Dongle**: `python -m nordicsemi dfu usb-serial -pkg target/central_dfu.zip -p COMx`

### Related Branches
- `feat/pointing-mode` — Per-layer pointing modes (Cursor/Scroll/Sniper), stashed TOML config work

---

## Development Workflow

### Core Principles

1. **Ask before acting** — When requirements are ambiguous, information is incomplete, or multiple valid approaches exist, **always ask the user for clarification before proceeding**. Do not assume or guess.
2. **Break large tasks into verifiable steps** — Decompose non-trivial tasks into small, independently verifiable units. Each step should have a clear completion criteria that can be checked before moving to the next.
3. **Cite real sources for standards and protocols** — When referencing hardware specs, communication protocols, SDK APIs, or any standardized content, always provide the actual source (datasheet section, official doc URL, SDK header path, RFC number, etc.). Never fabricate or guess technical details — verify from primary sources first.

### Change Size Classification

```
Small changes (bug fixes, parameter adjustments, single-function tweaks)
  → Direct modification → Explain changes → Self-verify

Large changes (refactoring, new features, multi-file architecture changes)
  → Plan mode → Discuss approach → Confirm → Implement → Self-verify
```

### Plan Mode Usage Guidelines

**When to use Plan Mode:**
- Adding new protocol or communication layer (e.g., Gazell split driver)
- Refactoring existing module architecture
- Changes spanning 3+ files
- Algorithm or protocol design requiring discussion
- Any change where multiple valid approaches exist

**Plan Mode Workflow:**
1. Enter Plan mode (use EnterPlanMode tool)
2. Explore codebase and propose implementation approach
3. Discuss and confirm approach with user
4. Exit Plan mode and begin implementation
5. Continuous self-verification during implementation

**Plan Document Standards:**
- Create plan documents in `docs/` directory
- Maintain both English and Chinese versions when the user requests it
- Include pseudocode for core logic (not just prose descriptions)
- Track version number (v1, v2, ...) for each review round
- Record review findings and fixes in the document's changelog

### Code Review Flywheel Effect

After each code review round, update this `CLAUDE.md` file:
1. **Record problem patterns** — Add to "Common Mistakes" section if a new category emerges
2. **Accumulate project-specific rules** — Update relevant standards sections
3. **Record architecture decisions** — Update "Current Work" or add to relevant section
4. **Fix verification gaps** — If a review caught something self-verification missed, add the check

This creates a feedback loop: each review makes future work more accurate.

### Self-Verification Process

After each code modification, execute the following verifications **before presenting the change as complete**:

#### 1. Formatting (Mandatory)
```bash
cargo fmt --all -- --check
```

#### 2. Lint (Mandatory)
```bash
cargo clippy --all-targets -- -D warnings
# For feature-gated code:
cargo clippy --manifest-path rmk/Cargo.toml --features "split,wireless_gazell" -- -D warnings
```

#### 3. Compilation (Mandatory)
```bash
# Host build (mock mode)
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell

# ARM cross-compile (if FFI code changed)
cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840

# Examples (MUST cd into directory)
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -
```

#### 4. Tests (Mandatory)
```bash
cargo test --manifest-path rmk/Cargo.toml --lib
```

#### 5. Feature Combination Check (When feature gates are involved)
```bash
# Verify all relevant feature combinations compile
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
cargo check --manifest-path rmk/Cargo.toml --features "split"
```

### Verification Failure Handling

**If any verification step fails:**
1. **Stop immediately** — do not commit or present code as complete
2. **Analyze failure cause** — read the full error output
3. **Fix the issue** — address root cause, not symptoms
4. **Re-verify** — run the full verification suite again

### Verification Report

After completing a non-trivial change, summarize:

```
## Verification Report
### Modifications
- File: <path>
- Change: <description>

### Results
- cargo fmt: Passed / Failed
- cargo clippy: Passed / Failed (N warnings)
- cargo check (host): Passed / Failed
- cargo build (ARM): Passed / Failed / Skipped
- cargo test: Passed / Failed (N tests)

### Ready to commit: Yes / No
```

---

## Code Standards

### Rust General Standards

#### 1. Formatting
- **Mandatory**: All code must pass `cargo fmt`
- **Check**: Use `cargo clippy` to eliminate warnings
- **Commands**:
  ```bash
  cargo fmt --all
  cargo clippy --all-targets --all-features
  ```

#### 2. Error Handling
- Use `Result<T, E>` instead of `unwrap()`
- Implement `Display` and `Debug` for custom error types
- Avoid `panic!()` in library code (dangerous in embedded)
- Prefer `?` operator for error propagation

#### 3. Async Code Standards
- Use Embassy's `async/await`
- Avoid blocking operations (no OS scheduler in embedded)
- Prefer channel communication over shared state

#### 4. Memory Management
- **Forbidden**: Don't use `Box`, `Vec`, `String` in no_std environments
- **Prefer**: Use fixed-size arrays and `heapless` containers
- **Check**: Ensure code compiles with `#![no_std]`

### Embedded Rust Specific Standards

#### 1. Dependency Management
- All dependencies must support `no_std`
- Use `default-features = false` in `Cargo.toml`

#### 2. Feature Gates
- Use optional features for large functionality blocks

```rust
#[cfg(feature = "wireless_gazell")]
mod gazell_impl;
```

#### 3. Stack Memory Control
- Avoid large stack allocations (embedded stack typically < 64KB)
- Use `static` for large buffers

### FFI / C Interop Standards (Gazell-specific)

#### 1. Safety Boundary
- All `unsafe` FFI calls must be wrapped in safe Rust functions
- Document safety invariants with `// SAFETY:` comments
- Validate inputs before passing to C code

#### 2. Conditional Compilation
- Use `#[cfg(feature = "wireless_gazell")]` for real FFI code
- Provide mock fallback for `#[cfg(not(feature = "wireless_gazell"))]`
- This allows `cargo test` and `cargo check` on host without Nordic SDK

#### 3. Build System
- `build.rs` must fail with clear error if `NRF5_SDK_PATH` is not set
- Skip compilation gracefully on non-ARM targets (for IDE support)

---

## Git Commit Standards

### Commit Message Format
```
<type>(<scope>): <subject>

<body>
```

**ALL commit messages MUST be in English**

### Types
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation update
- `refactor`: Refactoring (no behavior change)
- `test`: Test related
- `chore`: Build/toolchain updates
- `perf`: Performance optimization

### Scopes
- `gazell`: Gazell 2.4G wireless protocol
- `wireless`: Wireless transport layer
- `dongle`: USB dongle / receiver
- `pointing`: Pointing device / trackball logic
- `rmk`: RMK core
- `split`: Split keyboard
- `ble`: BLE functionality
- `usb`: USB functionality
- `examples`: Example code
- `config`: Config structs / TOML parsing
- `macro`: Code generation macros

### ❌ Prohibited in Commit Messages

**NEVER include Co-Authored-By lines in commit messages**

```bash
# ❌ FORBIDDEN - Do NOT include these lines
Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
Co-Authored-By: Claude <...>
```

This project does not use co-author attribution for AI assistance.

---

## RMK Architecture: Three-Layer Change Rule

RMK has a strict three-layer pipeline. **Any feature that touches config or codegen must be updated in ALL three layers at once.**

```
keyboard.toml
    ↓  parsed by
rmk-config/src/lib.rs   (config structs: JoystickConfig, Pmw3610Config, ...)
    ↓  consumed by
rmk-macro/src/codegen/  (code generation: adc.rs, pmw3610.rs, pmw33xx.rs, ...)
    ↓  generates
runtime Rust code        (JoystickProcessor, NrfAdc, PointingProcessor, ...)
```

### Checklist Before Implementing Any Cross-Cutting Change

When adding a field, changing a struct, or updating a function signature:

**Step 1 — Trace the full data flow**
- Who *produces* the data?
- Who *consumes* the data?
- Fix **both** producer and consumer, not just one side.

**Step 2 — Check all three layers**
- [ ] `rmk-config/src/lib.rs` — does the config struct expose the new field? Is `#[serde(default)]` needed?
- [ ] `rmk-macro/src/codegen/` — does the codegen pass the new field/param correctly?
- [ ] Runtime (`rmk/src/`) — is the struct/function updated?

**Step 3 — Search all call sites before changing a function signature or struct**

```bash
grep -r "StructName {" --include="*.rs" --include="*.md"
grep -r "fn_name(" --include="*.rs" --include="*.md"
```

**Step 4 — Update examples and documentation (the "fourth layer")**
- [ ] `examples/use_rust/` — all example crates that reference the changed API
- [ ] `docs/docs/main/docs/` — all `.md` files with code blocks using the changed API
- [ ] Doc comments (`/// # Example`) on the changed structs/functions

---

## Common Mistakes and Prohibitions

### ❌ Absolutely Forbidden

1. **Don't use std in no_std code**
2. **Don't panic in library code**
3. **Don't break backward compatibility** without explicit discussion
4. **Don't commit unformatted code** — must run `cargo fmt` before commit
5. **Don't add `Co-Authored-By: Claude ...`** to any commit

### ⚠️ Caution Required (Gazell-specific)

1. **Blocking FFI calls in async context** — `gz_send()` blocks until ACK or timeout (~6ms busy-wait). In Embassy async, this starves the executor and prevents other tasks (including USB) from running. **Confirmed: USB CDC becomes unresponsive when `gz_send()` is called at 10Hz alongside USB.** Must move Gazell send/recv to a dedicated task or use non-blocking approach.

2. **Nordic SDK path sensitivity** — `build.rs` constructs paths like `{NRF5_SDK_PATH}/components/proprietary_rf/gzll/gcc/`. If the SDK has nested directories (e.g., `nRF5_SDK_17.1.0_ddde560/` inside the extract dir), the path must point to the inner directory.

3. **Feature combination testing**
   ```bash
   # Host (no wireless feature, mock mode)
   cargo test --manifest-path rmk/Cargo.toml --lib --no-default-features --features "wireless_gazell" -- wireless
   # ARM cross-compile (real FFI)
   cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840
   ```

4. **`nrf_gzll_init()` resets ALL settings to defaults** — Every call to `nrf_gzll_init()` wipes all custom configuration (channel table, address, data rate, etc.). The `gz_set_mode()` function calls `nrf_gzll_init()` internally, so config must be re-applied afterwards. Fixed by saving config in `gz_state.saved_config` and calling `gz_apply_config()` after every reinit.

5. **Custom Gazell config vs defaults — NEEDS RE-TESTING** — As of 2026-03-12, `GazellConfig::low_latency()` custom settings appeared to fail while Nordic defaults worked. However, the P2 root cause turned out to be a missing `build.rs` in the example crate, which meant the Gazell C library was not linked at all. The custom config failure may have been a misdiagnosis — needs re-testing now that the build system is correct.

6. **`GazellTransport::recv_frame()` has an `initialized` guard** — The Rust wrapper checks `self.initialized` and returns `NotInitialized` if the transport wasn't initialized via `gazell.init()`. If you bypass the wrapper (e.g., using `gz_init_default()`), you must also bypass `recv_frame()` and call `gz_recv()` directly.

---

## Lessons Learned (Hardware Verification)

### 1. Always Read INFO_UF2.TXT Before Assuming Memory Layout

**Problem**: Assumed nice!nano had SoftDevice S140, set `FLASH ORIGIN = 0x26000`. Firmware was written to wrong address and never executed.

**Root cause**: The actual nice!nano had `SoftDevice: not found` in INFO_UF2.TXT. App should start at `0x1000`.

**Rule**: Before setting memory.x for any UF2-bootloader board:
```
1. Enter UF2 mode (double-click reset)
2. Read INFO_UF2.TXT on the drive
3. If "SoftDevice: not found" → FLASH ORIGIN = 0x1000
4. If "SoftDevice: S140 v6.1.1" → FLASH ORIGIN = 0x26000
```

### 2. RAM ORIGIN Must Be 0x20000008 on nRF52840 with MBR

The MBR reserves the first 8 bytes of RAM (`0x20000000-0x20000007`) for its forward interrupt vector. If `memory.x` uses `RAM ORIGIN = 0x20000000`, the `.data` section initialization overwrites MBR's reserved area, causing crashes on warm reset. Always use `0x20000008`.

### 3. Precompiled C Libraries Need ISR Bridges for cortex-m-rt

Nordic's `gzll_nrf52840_gcc.a` exports CMSIS-named ISR handlers (`RADIO_IRQHandler`, `TIMER2_IRQHandler`, `SWI0_EGU0_IRQHandler`). These are NOT automatically placed in the cortex-m-rt vector table. Must add bridge functions:

```rust
#[pac::interrupt]
fn RADIO() {
    unsafe { RADIO_IRQHandler() }
}
```

**Naming difference**: PAC uses `EGU0_SWI0`, C library uses `SWI0_EGU0_IRQHandler`. Same interrupt (IRQ #20), different naming convention.

### 4. HFCLK Must Be Explicitly Started Without USB

USB driver automatically starts HFCLK (32MHz crystal). Without USB, must start manually before Gazell init:
```rust
pac::CLOCK.tasks_hfclkstart().write_value(1);
while pac::CLOCK.events_hfclkstarted().read() != 1 {}
```

### 5. Diagnostic Counters Are Essential for Embedded Debugging

Without a debug probe, add counters at every layer:
- **ISR bridge level**: `AtomicU32` counters in `#[interrupt]` functions (R=RADIO count, S=SWI0 count)
- **C callback level**: `volatile uint32_t` counters in callbacks (rx_cb_count, rx_fetch_ok/fail)
- **Rust wrapper level**: Track return values and error codes

> **Note**: The original observation (`cb=0` with custom config, `cb=52` with defaults) was initially attributed to a config problem, but the P2 root cause turned out to be a missing `build.rs` that prevented the Gazell C library from being linked. The `cb=0` → `cb=52` transition likely reflected the build fix, not a config change. The general technique of layered counters remains valuable.

### 6. Don't Trust Rust Wrapper Return Values — Check the FFI Layer

`GazellTransport::recv_frame()` returned `NotInitialized` because `self.initialized` was `false` (Gazell was initialized via `gz_init_default()`, bypassing the Rust wrapper's `init()`). When debugging, test at each layer independently.

> **Note**: The original `cb=52, ok=52` observation that "C layer was working fine" may have been observed after the build.rs fix was applied, not before. The general lesson (check each layer independently) remains valid.

### 7. Edition 2024 Requires `unsafe extern "C"`

Rust edition 2024 requires `unsafe` keyword on `extern "C"` blocks:
```rust
// Edition 2024:
unsafe extern "C" {
    fn RADIO_IRQHandler();
}
// Older editions:
extern "C" {
    fn RADIO_IRQHandler();
}
```

### 8. `cargo build` Does Not Track `memory.x` Changes

Changing `memory.x` does not trigger recompilation. Must `cargo clean` first:
```bash
cargo clean && cargo build --release
```

### 9. nRF52840 Default Config Auto-Enables BLE — Must Override for Gazell

`rmk-config` merges user's `keyboard.toml` with `default_config/nrf52840.toml`, which contains `[ble] enabled = true` and `usb_enable = true`. For Gazell peripherals (no BLE, no USB), `get_communication_config()` returns `CommunicationConfig::Ble` instead of `None`, causing codegen to emit BLE stack init code (`nrf_sdc`, `build_sdc`, `Irqs`, etc.).

**Fix** (Phase 2): Early-return `CommunicationConfig::None` when `connection = "gazell"`.
**Refined** (Phase 3): Gazell central (dongle) needs USB, so the fix now returns `Usb(usb_info)` when `usb_enable=true` + Gazell, and `None` when `usb_enable=false` + Gazell. Only BLE is suppressed for Gazell (shared radio).

### 10. Codegen for Gazell nRF52 Peripheral Needs HFCLK + IRQ Priority Init

Unlike BLE (which handles clock/IRQ via MPSL), Gazell peripherals need explicit:
1. **HFCLK start** — `CLOCK.tasks_hfclkstart()` (no USB means no auto-start)
2. **IRQ priorities** — RADIO/TIMER2 at P0, EGU0_SWI0 at P1

These must be generated by the codegen (in `expand_split_peripheral_entry`) because `rmk` crate cannot depend on `embassy_nrf`.

### 11. Dynamic RADIO Dispatch: `bind_interrupts!` vs Manual ISR

When BLE (MPSL) and Gazell coexist, shared interrupts (RADIO, EGU0_SWI0) need runtime dispatch. Key findings:

1. **`bind_interrupts!` generates both ISR functions and `Binding` trait impls** — cannot use it for shared interrupts (would create duplicate ISR definitions).
2. **Solution**: Write ISR handlers manually with `#[pac::interrupt]`, then `unsafe impl Binding<...>` on a custom `MpslIrqs` struct to satisfy MPSL's compile-time constraints.
3. **MPSL raw handlers are public**: `nrf_mpsl::raw::MPSL_IRQ_RADIO_Handler()`, `MPSL_IRQ_TIMER0_Handler()`, `MPSL_IRQ_RTC0_Handler()` can be called directly from manual ISRs.
4. **`LowPrioInterruptHandler::on_interrupt()`** can be called via the `Handler` trait: `<mpsl::LowPrioInterruptHandler as Handler<typelevel::EGU0_SWI0>>::on_interrupt()`.
5. **critical-section conflict**: `nrf-mpsl`'s `critical-section-impl` feature and `cortex-m`'s `critical-section-single-core` feature both define `_critical_section_1_0_acquire`. Use MPSL's implementation (remove `critical-section-single-core` from cortex-m).
6. **`BINDGEN_EXTRA_CLANG_ARGS`** needed for nrf-sdc/nrf-mpsl: `--sysroot=/usr/lib/arm-none-eabi -I/usr/lib/gcc/arm-none-eabi/10.3.1/include`.

---

## Testing Standards

### Pre-commit Self-check
- [ ] Code formatted (`cargo fmt --all`)
- [ ] No Clippy warnings (`cargo clippy --all-targets`)
- [ ] Unit tests pass (mock mode on host)
- [ ] Documentation updated (if API changed)
- [ ] Commit message in English, follows conventional format
- [ ] No `Co-Authored-By: Claude` in commit
- [ ] No `println!` or debug-only code left in

---

## Working with Uncertainty

### When to Ask for Clarification

**Always ask the user when:**

1. **Requirements are ambiguous**
2. **Design decisions need input** — e.g., should the dongle support multiple paired keyboards?
3. **Breaking changes or compatibility concerns**
4. **Hardware-specific assumptions** — e.g., pin assignments, RF channel selection

---

## CI Troubleshooting

### Golden Rule: Never Trust Local Environment Alone

When CI fails but local checks pass, the environment differs. Common issues:
- `NRF5_SDK_PATH` not set in CI — Gazell FFI crate will fail
- ARM GCC not available — C shim compilation fails
- Feature flags mismatch between local and CI

### What NOT To Do

- Don't run same command 10 times locally hoping it fixes itself
- Don't create empty commits to "trigger CI rerun"
- Don't manually edit formatting — let `cargo fmt` do it
- Don't assume "works on my machine" means CI is wrong

---

## Version History

- 2026-02-24: Initial version for `feat/pointing-mode` branch
- 2026-03-02: Adapted for `feat/gazell-2g4` branch
  - Updated Current Work section for Gazell 2.4G wireless
  - Added Environment Setup section with SDK paths
  - Added FFI/C Interop Standards section
  - Added Gazell-specific cautions (blocking FFI, SDK path, elink_core dependency)
  - Added `gazell`, `wireless`, `dongle` commit scopes
  - Preserved universal standards (code, git, architecture rules)
- 2026-03-04: Added Development Workflow section
  - Change size classification (small vs large)
  - Plan mode usage guidelines and workflow
  - Code review flywheel effect (auto-update CLAUDE.md)
  - Self-verification process (cargo fmt/clippy/check/test/build)
  - Verification failure handling and report template
- 2026-03-06: Added Core Principles to Development Workflow
  - Ask before acting (clarify ambiguity before proceeding)
  - Break large tasks into verifiable steps
  - Cite real sources for standards and protocols
- 2026-03-11: Phase 1 completed, Phase 2 plan updated to v10
  - Updated Branch Progress Summary: Phase 1 hardware validation marked done
  - Added Phase 2 progress table with Steps 1-13
  - Recorded architecture decisions (left-hand first, dongle as use_rust, multi-pipe final target)
  - Plan document expanded with Steps 9-13 (codegen, rmk-config, dongle USB HID, Charybdis integration)
- 2026-03-12: Hardware verification session — major lessons learned
  - Discovered nice!nano has NO SoftDevice (FLASH ORIGIN = 0x1000, not 0x26000)
  - Fixed `gz_set_mode()` config-loss bug (nrf_gzll_init resets all settings)
  - Confirmed ISR bridges work (RADIO/TIMER2/EGU0_SWI0 all in vector table)
  - Confirmed Gazell radio link works with Nordic defaults (cb=52, ok=52)
  - Identified custom config as source of communication failure (needs bisection)
  - Added 8 lessons learned to Lessons Learned section
  - Updated Caution Required section with confirmed issues
- 2026-03-14: Phase 3 multi-pipe Gazell implementation (Steps 1-7 software complete)
  - GazellCentralHub + PipeDriver: single gz_recv() owner dispatches to per-pipe channels
  - DummyMatrix for zero-matrix central (USB dongle with no keys)
  - ISR bridge codegen for both central and peripheral Gazell boards
  - CommunicationConfig fix: Gazell central gets Usb(...), peripheral gets None
  - `_wireless` feature gate for BatteryState (shared by BLE and Gazell)
  - Charybdis 3-binary example (central + left + right) all build on ARM
  - Architecture decision revised: dongle now uses keyboard.toml codegen (#[rmk_central])
  - Annotated lessons 5, 6 and caution 5: P2 "custom config failure" was likely caused by missing build.rs
- 2026-03-25: Phase 4 BLE/Gazell hot-switching — preparatory work
  - Added cancel tokens (`GAZELL_HUB_CANCEL`, `GAZELL_PERIPHERAL_CANCEL`) with graceful shutdown
  - PipeDriver: poison pill via `Option<SplitMessage>` channel, event-driven (zero polling overhead)
  - Peripheral: cancel-aware `read()` with `signaled()` (non-consuming), deep idle `pending()` preserved for power
  - Storage gate analysis: `storage + _ble` gate is BLE-only (PeerAddress), Gazell Vial already works
  - `compile_error!` analysis: cannot simply remove — `peripheral.rs` / `central.rs` use `#[cfg]` on individual generic params, requires enum dispatch refactor
  - Branch renamed to `feat/ble-gazell-switch`
- 2026-03-26: Rebase onto upstream/main (134 commits ahead)
  - Created `feat/gazell-rebase` from `upstream/main`, manually ported all Gazell code
  - Old branch `feat/ble-gazell-switch` preserved as reference
  - Adapted to upstream refactors: rmk-macro rewrite, rmk-config resolved layer, event system merge
  - Re-implemented codegen on new macro architecture (entry.rs, split/central.rs, split/peripheral.rs, bind_interrupt.rs)
  - Split peripheral/central entry functions (`_ble`, `_gazell`, `_serial`) for BLE+Gazell coexistence
  - Compile-verified: all feature combinations pass, 110 unit tests pass
- 2026-04-03: Phase 4.1 Dynamic RADIO Interrupt Dispatcher — HW verified
  - Created `examples/use_rust/nrf52840_radio_switch_poc/` — standalone PoC firmware
  - Dynamic ISR dispatch via `AtomicU8` (RADIO_MODE / EGU0_MODE) — runtime switching between Gazell and BLE
  - Manual `unsafe impl Binding` to satisfy MPSL type constraints without `bind_interrupts!` for shared IRQs
  - USB CDC logging via `embassy-usb-logger` (no debug probe needed)
  - HW verified on E104-BT5040U dongle: Gazell x2 → BLE advertise (visible on nRF Connect) → Gazell x1, zero crashes
  - Resolved: critical-section link conflict (nrf-mpsl vs cortex-m), BINDGEN_EXTRA_CLANG_ARGS for ARM cross-compile
  - Binary size: 87KB (Gazell + BLE + USB CDC in one firmware)
