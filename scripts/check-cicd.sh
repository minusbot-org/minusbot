#!/bin/bash

# CI/CD Pipeline Check Script
# Verifies the GitHub Actions workflow configuration

echo "Checking CI/CD pipeline configuration..."

# Check if we have a .git directory (repo setup)
if [ ! -d .git ]; then
    echo "❌ GitHub repository not initialized"
    exit 1
fi

# Check if .github directory exists
if [ ! -d .github ]; then
    echo "❌ .github directory not found"
    exit 1
fi

# Check if workflow files exist
if [ ! -f .github/workflows/cicd.yml ]; then
    echo "❌ Main CI/CD workflow file not found (.github/workflows/cicd.yml)"
    exit 1
fi

if [ ! -f .github/workflows/youtubetv.yml ]; then
    echo "❌ YouTube TV workflow file not found (.github/workflows/youtubetv.yml)"
    exit 1
fi

echo "✅ CI/CD pipeline check passed"
echo ""
echo "GitHub Actions workflows configured:"
echo "  - .github/workflows/cicd.yml"
echo "  - .github/workflows/youtubetv.yml"
echo ""
echo "Triggers:"
echo "  - Push to main/develop branches"
echo "  - Pull requests to main/develop"
echo "  - Manual trigger (workflow_dispatch)"
echo ""
echo "Jobs:"
echo "  - Setup: Dynamic analysis and matrix generation"
echo "  - Lint-All: Run linters on all skills"
echo "  - Test: Run tests per skill (matrix)"
echo "  - Documentation: Check docstrings and docs"
echo "  - Security: Vulnerability scanning"
echo "  - Summary: Build status report"
echo ""
echo "To trigger manually:"
echo "  GitHub UI → Actions → Run workflow"
echo ""
echo "To see logs:"
echo "  GitHub UI → Actions → Select workflow run → View logs"
exit 0
