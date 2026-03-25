# Rebase Plan: Port Gazell to Upstream Main

> **Status**: In Progress
> **Created**: 2026-03-25
> **Branch**: `feat/gazell-rebase` (from `upstream/main`)
> **Old Branch**: `feat/ble-gazell-switch` (38 commits, preserved as reference)

## Background

Upstream (`HaoboGu/rmk` main) is 134 commits ahead of our fork, including major refactors:

| Upstream Change | Impact on Gazell |
|----------------|-----------------|
| rmk-macro **full rewrite** | All codegen must be re-implemented |
| rmk-config **resolved layer** | Config structs restructured |
| Event system **merge** (controller + input) | Import paths changed |
| KeyMap **de-generics** (#763) | Minor, our code doesn't depend on KeyMap generics |
| Storage key format optimization | No direct impact |
| sequential-storage v7.1.0 | No direct impact |

**53 files conflict** between our branch and upstream. Direct `git rebase` is infeasible. Strategy: **fresh branch from upstream/main, manual port.**

## Code Classification

### A: Zero-conflict new files (copy directly)

```
rmk-gazell-sys/                          # FFI crate, fully independent
rmk/src/split/gazell.rs                  # Gazell split driver
rmk/src/wireless/{config.rs, mod.rs}     # GazellConfig + module
examples/use_config/nrf52840_gazell_split/   # Codegen example
examples/use_rust/nrf52840_{dongle,2g4}/     # Standalone examples
docs/{GAZELL_*,ble_gazell_*,plan-*,phase2-*,gazell-power-*}.md
CLAUDE.md, CLAUDE.zh.md
```

### B: Small edits on upstream files (manual merge)

```
rmk/Cargo.toml                    # Add wireless_gazell feature + deps
rmk/src/split/mod.rs              # Add gazell module + BatteryState ungate
rmk/src/split/peripheral.rs       # Add Gazell entry function
rmk/src/split/central.rs          # Add Gazell entry function (if needed)
rmk/src/split/driver.rs           # Add GAZELL_MAX_PAYLOAD const
rmk/src/lib.rs                    # Add wireless module export
rmk-config/src/lib.rs             # Add GazellSplitConfig, gazell_pipe
rmk-config/src/communication.rs   # Gazell CommunicationConfig handling
rmk/src/matrix.rs                 # DummyMatrix for zero-matrix dongle
rmk/src/driver/flex_pin/mod.rs    # wireless_gazell feature gate
```

### C: Re-implement on new upstream architecture

```
rmk-macro/src/codegen/entry.rs              # Gazell task spawning
rmk-macro/src/codegen/split/central.rs      # Gazell central init codegen
rmk-macro/src/codegen/split/peripheral.rs   # Gazell peripheral codegen
rmk-macro/src/codegen/chip/bind_interrupt.rs # Gazell ISR bridges
```

## Execution Steps

### Step 1: Create new branch (~5min)

```bash
git fetch upstream main
git checkout -b feat/gazell-rebase upstream/main
```

### Step 2: Port A-class files (~10min)

```bash
git checkout feat/ble-gazell-switch -- rmk-gazell-sys/
git checkout feat/ble-gazell-switch -- rmk/src/split/gazell.rs
git checkout feat/ble-gazell-switch -- rmk/src/wireless/
git checkout feat/ble-gazell-switch -- examples/use_config/nrf52840_gazell_split/
git checkout feat/ble-gazell-switch -- examples/use_rust/nrf52840_dongle/
git checkout feat/ble-gazell-switch -- examples/use_rust/nrf52840_2g4/
git checkout feat/ble-gazell-switch -- docs/GAZELL_*.md docs/ble_gazell_*.md ...
git checkout feat/ble-gazell-switch -- CLAUDE.md CLAUDE.zh.md
```

**Verify**: `cargo check -p rmk-gazell-sys --target thumbv7em-none-eabihf --features nrf52840`
**Commit**: `feat(gazell): port Gazell FFI crate, split driver, and examples`

### Step 3: Adapt B-class files (~1-2h)

For each file: read upstream new version → find insertion point → add our changes.

- 3a. `rmk/Cargo.toml` — wireless_gazell feature + rmk-gazell-sys dep
- 3b. `rmk/src/split/mod.rs` — `pub mod gazell;`, BatteryState → `any(_ble, wireless_gazell)`
- 3c. `rmk/src/split/peripheral.rs` — Add `run_rmk_split_peripheral_gazell()`
- 3d. `rmk/src/split/central.rs` — Add Gazell entry if needed (or skip, codegen calls gazell.rs directly)
- 3e. `rmk/src/split/driver.rs` — `GAZELL_MAX_PAYLOAD` const
- 3f. `rmk/src/lib.rs` — `pub mod wireless;`
- 3g. `rmk-config/src/lib.rs` — GazellSplitConfig, gazell_pipe on new resolved layer
- 3h. `rmk-config/src/communication.rs` — Gazell CommunicationConfig
- 3i. `rmk/src/matrix.rs` — DummyMatrix
- 3j. `rmk/src/driver/flex_pin/mod.rs` — feature gate

**Verify**: `cargo check -p rmk --features "split,wireless_gazell"`
**Commit**: `feat(gazell): integrate Gazell into upstream split/config/driver`

### Step 4: Re-implement codegen on new macro arch (~2-3h)

Read upstream BLE/Serial branches as templates, add `"gazell"` arms at same locations.

1. `entry.rs` — `"gazell"` connection arm (hub task + pipe manager tasks)
2. `split/central.rs` — `"gazell"` match arm (HFCLK + IRQ + gz_init_default)
3. `split/peripheral.rs` — `"gazell"` branch (HFCLK + IRQ + gz_init + entry fn call)
4. `chip/bind_interrupt.rs` — Short-circuit before BLE config access when connection == "gazell"

**Verify**: `cargo check -p rmk-macro`
**Commit**: `feat(gazell): add Gazell codegen for split central and peripheral`

### Step 5: Fix API changes in gazell.rs (~30min)

Upstream event system merged — adapt imports:

| Old | New |
|-----|-----|
| `SubscribableInputEvent` | `SubscribableEvent` |
| `publish_controller_event` | `publish_event` |
| `TouchpadEvent` | Removed (check if used) |
| `SubscribableControllerEvent` | `SubscribableEvent` |

Compile → fix → repeat until passing.

**Verify**: `cargo check -p rmk --features "split,wireless_gazell"` + `cargo test -p rmk --lib`
**Commit**: `refactor(gazell): adapt to upstream event system and API changes`

### Step 6: Re-apply Phase 4 changes (~15min)

- Remove `compile_error!` in `split/mod.rs`
- Split `run_rmk_split_peripheral` into transport-specific functions
- Update codegen call sites

**Verify**: `cargo check -p rmk --features "split,_ble,wireless_gazell"`
**Commit**: `refactor(split): split entry functions for BLE/Gazell coexistence`

## Final Verification

```bash
cargo fmt --all -- --check
cargo check -p rmk --features "split,_ble"
cargo check -p rmk --features "split,wireless_gazell"
cargo check -p rmk --features "split,_ble,wireless_gazell"
cargo test -p rmk --lib
cargo test -p rmk-macro
cargo clippy -p rmk --features "split,wireless_gazell" -- -D warnings
cargo build -p rmk-gazell-sys --target thumbv7em-none-eabihf --features nrf52840
cargo test --workspace   # Upstream tests must not regress
```

## Estimate

| Step | Time | Risk |
|------|------|------|
| Step 1-2: Setup + A-class | 15min | Low |
| Step 3: B-class adaptation | 1-2h | Medium |
| Step 4: C-class codegen rewrite | 2-3h | **High** |
| Step 5: API adaptation | 30min | Low |
| Step 6: Phase 4 re-apply | 15min | Low |
| **Total** | **4-6h** | |

## Changelog

| Date | Update |
|------|--------|
| 2026-03-25 | Initial plan created |
