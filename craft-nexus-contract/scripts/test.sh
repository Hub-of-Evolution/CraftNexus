#!/bin/bash
set -e

echo "🧪 Running contract tests..."

echo "Checking code with cargo check..."
cargo check --tests

echo "Running clippy lint checks..."
cargo clippy -- -D warnings

echo "Running tests..."
cargo test --release

echo "✅ All tests and checks passed!"
