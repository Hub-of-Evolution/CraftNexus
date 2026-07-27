#!/bin/bash
# Setup script for pre-commit hooks
# This script installs pre-commit hooks that run cargo fmt and eslint checks

set -e

echo "🔧 Setting up pre-commit hooks for CraftNexus..."

# Check if pre-commit is installed
if ! command -v pre-commit &> /dev/null; then
    echo "📦 pre-commit is not installed. Installing..."
    
    # Check if pip is available
    if command -v pip3 &> /dev/null; then
        pip3 install pre-commit
    elif command -v pip &> /dev/null; then
        pip install pre-commit
    else
        echo "❌ pip is not installed. Please install pip first:"
        echo "   - Ubuntu/Debian: sudo apt install python3-pip"
        echo "   - macOS: brew install python3"
        echo "   - Windows: https://pip.pypa.io/en/stable/installation/"
        exit 1
    fi
else
    echo "✅ pre-commit is already installed"
fi

# Install the git hook
echo "🔨 Installing pre-commit hooks..."
pre-commit install

# Run against all files to ensure everything is formatted correctly
echo "🧹 Running pre-commit on all files..."
pre-commit run --all-files || true

echo ""
echo "✅ Pre-commit hooks setup complete!"
echo ""
echo "The following checks will now run before each commit:"
echo "  - cargo fmt --check (for Rust files)"
echo "  - eslint (for TypeScript/TSX files)"
echo "  - trailing-whitespace removal"
echo "  - end-of-file fixer"
echo "  - YAML validation"
echo "  - large file check"
echo "  - merge conflict detection"
echo "  - private key detection"
echo ""
echo "To run checks manually without committing:"
echo "  pre-commit run --all-files"
echo ""
echo "To skip hooks for a specific commit (not recommended):"
echo "  git commit --no-verify"