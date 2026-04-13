#!/bin/bash

# Linting script for Minusbot
# Runs ruff and black on all skills

echo "Running linter checks..."

# Run ruff on all Python files
ruff check skills/ 2>&1

LINT_RESULT=$?

if [ $LINT_RESULT -eq 0 ]; then
    echo "All checks passed!"
else
    echo "❌ Linting failed"
    exit 1
fi

echo "Running formatter checks..."

# Run black on all Python files
black --check skills/ 2>&1

FORMAT_RESULT=$?

if [ $FORMAT_RESULT -eq 0 ]; then
    echo "All done! ✨ 🍰 ✨"
    echo "All checks passed!"
else
    echo "Oh no! 💥 💔 💥"
    echo "Some files would be reformatted."
    exit 1
fi
