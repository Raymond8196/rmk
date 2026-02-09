# Skill: build-examples

Build all RMK examples to verify Elink integration doesn't break existing functionality.

## Usage
```
/build-examples [--target <mcu>]
```

## What This Skill Does

1. **Enumerate all examples** in the examples directory
2. **Build each example** with appropriate target
3. **Check binary sizes** to ensure no excessive bloat
4. **Report compilation results**

## Implementation

### Step 1: List all examples
```bash
cd examples/use_rust
ls -d */ | grep -v target
```

### Step 2: Build STM32 examples
```bash
cd examples/use_rust/stm32h7
cargo build --release --target thumbv7em-none-eabihf
```

### Step 3: Build nRF52 examples
```bash
cd examples/use_rust/nrf52840_ble_split_dongle
cargo build --release --target thumbv7em-none-eabihf
```

### Step 4: Build RP2040 examples
```bash
cd examples/use_rust/rp2040
cargo build --release --target thumbv6m-none-eabi
```

### Step 5: Check binary sizes
```bash
cargo size --release --target <target> -- -A
```

## Expected Output

- All examples compile successfully ✅
- Binary sizes within reasonable limits (< 128KB for typical keyboards) ✅
- No warnings about memory overflow ✅

## Troubleshooting

- If build fails: Check dependency compatibility
- If binary too large: Use `--no-default-features` to reduce size
- If linker errors: Verify memory.x configuration
