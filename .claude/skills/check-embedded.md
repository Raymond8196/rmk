# Skill: check-embedded

Verify no_std compatibility and embedded readiness of Elink code.

## Usage
```
/check-embedded
```

## What This Skill Does

1. **Check no_std compilation** for elink-core
2. **Verify no heap allocations** in critical paths
3. **Analyze stack usage** with cargo-call-stack
4. **Check for panic paths** that could brick embedded devices
5. **Validate buffer sizes** are reasonable for embedded

## Implementation

### Step 1: Verify no_std compilation
```bash
cd elink-protocol/elink-core
cargo check --no-default-features --target thumbv7em-none-eabihf
```

### Step 2: Search for forbidden std usage
```bash
grep -r "use std::" elink-protocol/ --include="*.rs" | grep -v "cfg(test)"
grep -r "Box\|Vec<.*>=" elink-protocol/ --include="*.rs" | grep -v "cfg(test)"
```

### Step 3: Check for panic macros
```bash
grep -r "panic!\|unwrap()\|expect(" elink-protocol/ --include="*.rs" | grep -v "cfg(test)"
```

### Step 4: Verify buffer sizes
```bash
grep -r "u8;.*\]" elink-protocol/ --include="*.rs" | grep -E "\[[0-9]{4,}\]"
```

### Step 5: Run clippy with embedded checks
```bash
cargo clippy --target thumbv7em-none-eabihf -- \
  -W clippy::panic \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::large_stack_arrays
```

## Expected Output

- No std usage in non-test code ✅
- No panic/unwrap in library code ✅
- Buffer sizes < 1KB each ✅
- All clippy checks pass ✅

## Troubleshooting

- If std found: Replace with heapless or custom no_std alternative
- If panic found: Refactor to return Result<T, E>
- If large buffers: Consider using static allocation or reducing size
