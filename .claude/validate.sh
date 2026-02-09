#!/bin/bash
set -e

LEVEL=${1:-2}  # Default to Level 2
PROJECT_ROOT=$(cd "$(dirname "$0")/.." && pwd)

cd "$PROJECT_ROOT"

echo "🔍 Running validation level $LEVEL..."

if [ "$LEVEL" -ge 1 ]; then
  echo "📝 Level 1: Syntax and Format"
  cargo fmt --all --check || {
    echo "❌ Format check failed. Run: cargo fmt --all"
    exit 1
  }
  cargo clippy --all-targets --all-features -- -D warnings || {
    echo "❌ Clippy check failed"
    exit 1
  }
  cargo check --all-features || {
    echo "❌ Compilation check failed"
    exit 1
  }
  echo "✅ Level 1 passed"
fi

if [ "$LEVEL" -ge 2 ]; then
  echo "🧪 Level 2: Functional Correctness"
  cargo test --all-features || {
    echo "❌ Tests failed"
    exit 1
  }
  echo "✅ Level 2 passed"
fi

if [ "$LEVEL" -ge 3 ]; then
  echo "🔌 Level 3: Embedded Compatibility"

  # Check elink-core no_std compilation
  if [ -d "elink-protocol/elink-core" ]; then
    cd elink-protocol/elink-core
    cargo check --no-default-features --target thumbv7em-none-eabihf || {
      echo "❌ Embedded compilation failed"
      exit 1
    }
    cd "$PROJECT_ROOT"
  fi

  # Check for unwrap/panic in non-test code
  UNWRAPS=$(grep -r "\.unwrap()\|\.expect(\|panic!" rmk/src elink-protocol/ --include="*.rs" 2>/dev/null | grep -v "test" | grep -v "//" | wc -l || echo 0)
  if [ "$UNWRAPS" -gt 0 ]; then
    echo "⚠️  Found $UNWRAPS unwrap/panic instances in non-test code"
    grep -r "\.unwrap()\|\.expect(\|panic!" rmk/src elink-protocol/ --include="*.rs" 2>/dev/null | grep -v "test" | grep -v "//"
    exit 1
  fi

  echo "✅ Level 3 passed"
fi

if [ "$LEVEL" -ge 4 ]; then
  echo "⚡ Level 4: Performance Benchmarks"

  if [ -d "elink-protocol/elink-rmk-adapter" ]; then
    cd elink-protocol/elink-rmk-adapter
    if cargo run --example benchmark --release 2>&1 | tee "$PROJECT_ROOT/.claude/benchmark.log"; then
      cd "$PROJECT_ROOT"
      echo "✅ Level 4 passed"
    else
      echo "❌ Benchmark failed"
      exit 1
    fi
  else
    echo "⚠️  Elink adapter not found, skipping benchmarks"
  fi
fi

if [ "$LEVEL" -ge 5 ]; then
  echo "🏗️  Level 5: Full Build Matrix"

  cargo test --no-default-features || {
    echo "❌ No-default-features test failed"
    exit 1
  }

  cargo test --features split || {
    echo "❌ Split feature test failed"
    exit 1
  }

  if cargo test --features split,elink 2>&1; then
    echo "✅ Split+Elink features passed"
  else
    echo "⚠️  Elink feature not available, skipping"
  fi

  echo "✅ Level 5 passed"
fi

echo ""
echo "🎉 All validations passed!"
echo ""
