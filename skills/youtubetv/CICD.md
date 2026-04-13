# Minusbot YouTube TV - CI/CD Pipeline

## Overview

This document describes the CI/CD pipeline for the Minusbot YouTube TV skill.

## Pipeline Structure

The GitHub Actions pipeline runs on:
- **Push** to `main` or `develop` branches (when files in `skills/youtubetv/` change)
- **Pull requests** targeting `main` or `develop` (when files in `skills/youtubetv/` change)
- **Manual triggers** via GitHub UI

## Jobs

### 1. Test Job
Runs comprehensive tests:
- pytest with verbose output
- Test coverage for wrapper.py functionality

**Steps:**
1. Checkout code
2. Set up Python 3.12
3. Install dependencies (runtimes + test tools)
4. Run tests: `pytest -v --tb=short`

### 2. Lint Job
Runs linters on all skills:
- ruff (linter)
- black (formatter)
- Custom lint script

**Steps:**
1. Checkout code
2. Set up Python
3. Install ruff
4. Run: `bash scripts/lint.sh`

### 3. Documentation Job
Checks documentation quality:
- Docstring coverage
- Sphinx documentation generation

**Steps:**
1. Checkout code
2. Set up Python
3. Install Sphinx + theme
4. Run pydoclint for docstring verification

## Configuration Files

### `.github/workflows/youtubetv.yml`
Main CI/CD configuration file

### `skills/youtubetv/scripts/pyproject.toml`
Project dependencies and tool configurations

### `skills/youtubetv/scripts/ruff.toml`
Linter configuration

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PYTHON_VERSION` | Python version to use | `3.12` |
| `LOG_LEVEL` | Default log level for tests | `INFO` |

## Test coverage

Current test suite covers:
- `load_favorites()` - 4 tests
- `save_favorites()` - 2 tests
- `resolve_client()` - 2 tests
- `handle_discover()` - 1 test
- `handle_save_favorite()` - 1 test
- `handle_list_saved()` - 2 tests
- `handle_control()` - 3 tests

**Total: 19 passing tests**

## Build Process

No build step required - Python scripts are executed directly.

## Deployment

Manual deployment via:
```bash
bash scripts/deploy.sh
```

## Troubleshooting

### Tests failing
- Check `LOG_LEVEL` environment variable
- Ensure all dependencies are installed
- Review test output for specific failures

### Lint errors
- Run: `ruff check . --fix`
- Run: `black .`
- Check ruff.toml for configuration

### Type errors
- Install mypy: `pip install mypy`
- Run: `mypy wrapper.py logging_utils.py --ignore-missing-imports`
