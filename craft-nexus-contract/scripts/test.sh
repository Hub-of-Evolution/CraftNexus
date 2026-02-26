#!/bin/bash
set -e

echo "🧪 Running contract tests..."

cargo test --release

echo "✅ All tests passed!"
