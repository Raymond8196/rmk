# Project Validation Framework

> "Give Claude the means to validate its own work. If Claude can verify its work, the final product quality improves 2-3x. Investing effort in validation mechanisms yields the highest ROI." - Boris Cherny Tip #13

## Philosophy

Claude should be able to **autonomously validate** changes before declaring work complete. This document defines validation criteria and automated checks.

## Validation Levels

### Level 1: Syntax and Format (Fast, < 10s)
**Run on every code change**

```bash
# Format check
cargo fmt --all --check

# Clippy lints
cargo clippy --all-targets --all-features -- -D warnings

# Basic compilation
cargo check --all-features
```

**Success Criteria**:
- ✅ No formatting issues
- ✅ Zero clippy warnings
- ✅ Compiles without errors

### Level 2: Functional Correctness (Medium, < 2min)
**Run before committing**

```bash
# Unit tests
cargo test --all-features

# Integration tests
cargo test --features split,elink --test '*'

# Doc tests
cargo test --doc
```

**Success Criteria**:
- ✅ All tests pass
- ✅ No test failures or panics
- ✅ Code coverage > 70% for new code

### Level 3: Embedded Compatibility (Medium, < 1min)
**Run for embedded-related changes**

```bash
# no_std compilation check
cd elink-protocol/elink-core
cargo check --no-default-features --target thumbv7em-none-eabihf

# Check for forbidden patterns
grep -r "unwrap()\|expect(\|panic!" src/ --include="*.rs" | grep -v "test"

# Verify buffer sizes
grep -E "u8; [0-9]{4,}" src/ -r
```

**Success Criteria**:
- ✅ Compiles for embedded target
- ✅ No unwrap/panic in library code
- ✅ Buffer sizes < 2KB

### Level 4: Performance Benchmarks (Slow, < 5min)
**Run for protocol changes**

```bash
cd elink-protocol/elink-rmk-adapter
cargo run --example benchmark --release
```

**Success Criteria**:
- ✅ Encoding latency < 15µs
- ✅ Decoding latency < 15µs
- ✅ Memory overhead < 1KB
- ✅ CPU usage < 1% @ 200 msg/s

### Level 5: Full Build Matrix (Slowest, < 10min)
**Run before PR submission**

```bash
# Test all feature combinations
cargo test --no-default-features
cargo test --features split
cargo test --features split,elink
cargo test --all-features

# Build all examples
cd examples/use_rust
for dir in */; do
  cd "$dir"
  cargo build --release 2>&1 | tee build.log
  cd ..
done

# Check binary sizes
cargo size --release -- -A
```

**Success Criteria**:
- ✅ All feature combinations compile
- ✅ All examples build successfully
- ✅ Binary sizes reasonable (< 128KB)

## Automated Validation Script

Create this script: `.claude/validate.sh`

```bash
#!/bin/bash
set -e

LEVEL=${1:-2}  # Default to Level 2

echo "🔍 Running validation level $LEVEL..."

if [ "$LEVEL" -ge 1 ]; then
  echo "📝 Level 1: Syntax and Format"
  cargo fmt --all --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo check --all-features
  echo "✅ Level 1 passed"
fi

if [ "$LEVEL" -ge 2 ]; then
  echo "🧪 Level 2: Functional Correctness"
  cargo test --all-features
  echo "✅ Level 2 passed"
fi

if [ "$LEVEL" -ge 3 ]; then
  echo "🔌 Level 3: Embedded Compatibility"
  cd elink-protocol/elink-core
  cargo check --no-default-features --target thumbv7em-none-eabihf
  cd ../..

  UNWRAPS=$(grep -r "unwrap()\|expect(\|panic!" rmk/src elink-protocol/ --include="*.rs" | grep -v "test" | wc -l)
  if [ "$UNWRAPS" -gt 0 ]; then
    echo "⚠️  Found $UNWRAPS unwrap/panic instances in non-test code"
    exit 1
  fi
  echo "✅ Level 3 passed"
fi

if [ "$LEVEL" -ge 4 ]; then
  echo "⚡ Level 4: Performance Benchmarks"
  cd elink-protocol/elink-rmk-adapter
  cargo run --example benchmark --release | tee benchmark.log
  cd ../..

  # Parse benchmark results (simplified)
  if grep -q "FAIL" benchmark.log; then
    echo "❌ Benchmark failed"
    exit 1
  fi
  echo "✅ Level 4 passed"
fi

if [ "$LEVEL" -ge 5 ]; then
  echo "🏗️  Level 5: Full Build Matrix"
  cargo test --no-default-features
  cargo test --features split
  cargo test --features split,elink
  echo "✅ Level 5 passed"
fi

echo "🎉 All validations passed!"
```

## Usage in Claude Workflow

### When to Run Validation

1. **After editing code** → Run Level 1
2. **Before committing** → Run Level 2
3. **After protocol changes** → Run Level 4
4. **Before creating PR** → Run Level 5

### Example Validation Flow

```bash
# Claude makes changes to elink adapter
# Step 1: Claude runs Level 1 validation
./claude/validate.sh 1

# If pass, Claude runs Level 2
./claude/validate.sh 2

# If protocol changes, Claude runs Level 4
./claude/validate.sh 4

# Claude reports results to user
echo "Validation complete: All checks passed ✅"
```

## Self-Validation Checklist for Claude

Before declaring work "complete", Claude must verify:

### Code Quality
- [ ] Ran `cargo fmt --check` - no formatting issues
- [ ] Ran `cargo clippy` - zero warnings
- [ ] All unit tests pass
- [ ] Integration tests pass (if applicable)

### Documentation
- [ ] Public APIs have doc comments
- [ ] Examples added for complex features
- [ ] CLAUDE.md updated if new patterns introduced
- [ ] All text in English (no Chinese in docs/comments/commits)

### Embedded Compatibility
- [ ] Code compiles for embedded targets
- [ ] No `unwrap()` in library code
- [ ] No `panic!()` in library code
- [ ] Buffer sizes reasonable (< 2KB per buffer)
- [ ] No std library usage in no_std code

### Performance
- [ ] Benchmarks run successfully
- [ ] Latency within acceptable range
- [ ] Memory overhead acceptable
- [ ] No obvious performance regressions

### Git Standards
- [ ] Commit message in English
- [ ] Follows conventional commit format
- [ ] Includes detailed body explaining changes
- [ ] References related issues if applicable

## Reporting Validation Results

Claude should report validation results in this format:

```
## Validation Report

### Level 1: Syntax and Format ✅
- cargo fmt: Pass
- cargo clippy: Pass (0 warnings)
- cargo check: Pass

### Level 2: Functional Correctness ✅
- Unit tests: 47/47 passed
- Integration tests: 12/12 passed
- Doc tests: 5/5 passed

### Level 3: Embedded Compatibility ✅
- Embedded compilation: Pass
- No unwrap/panic: Verified
- Buffer sizes: All < 1KB

### Performance Metrics
- Encoding latency: 10.2µs ✅
- Decoding latency: 12.1µs ✅
- Memory overhead: 760 bytes ✅

### Summary
All validations passed. Ready for commit.
```

## Continuous Improvement

### When Validation Fails

1. **Don't ignore failures** - Fix the issue immediately
2. **Update validation rules** if new failure patterns emerge
3. **Document workarounds** in CLAUDE.md if necessary

### Adding New Validation Rules

When adding new rules:
1. Update this document
2. Update `.claude/validate.sh` script
3. Add to pre-commit checklist in CLAUDE.md
4. Document expected behavior

## Integration with CI/CD

This validation framework should mirror CI/CD checks:

```yaml
# Example .github/workflows/ci.yml structure
- Format check (Level 1)
- Clippy lints (Level 1)
- Unit tests (Level 2)
- Integration tests (Level 2)
- Embedded build (Level 3)
- Benchmarks (Level 4)
- Example builds (Level 5)
```

## Version History
- 2026-02-09: Initial validation framework
  - Defined 5 validation levels
  - Created validation script template
  - Established self-validation checklist
