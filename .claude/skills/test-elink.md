# Skill: test-elink

Test the Elink protocol implementation comprehensively.

## Usage
```
/test-elink [--verbose]
```

## What This Skill Does

1. **Run unit tests** for elink-core and elink-rmk-adapter
2. **Run integration tests** for the RMK split module with Elink
3. **Check for compilation** with different feature combinations
4. **Generate test report** with coverage and performance metrics

## Implementation

### Step 1: Test elink-core
```bash
cd elink-protocol/elink-core
cargo test --all-features
cargo test --no-default-features
```

### Step 2: Test elink-rmk-adapter
```bash
cd elink-protocol/elink-rmk-adapter
cargo test --all-features
cargo run --example pc_test --release
```

### Step 3: Test RMK integration
```bash
cd rmk
cargo test --features split,elink
cargo check --features split,elink --target thumbv7em-none-eabihf
```

### Step 4: Run benchmarks
```bash
cd elink-protocol/elink-rmk-adapter
cargo run --example benchmark --release
```

## Expected Output

- All tests pass ✅
- No clippy warnings ✅
- Benchmark shows < 25µs latency ✅
- Memory usage < 1KB ✅

## Troubleshooting

- If tests fail: Check CLAUDE.md for error handling standards
- If compilation fails: Verify no_std compatibility
- If benchmarks show high latency: Check buffer sizes and unnecessary allocations
