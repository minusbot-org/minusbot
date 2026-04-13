# Logging System - Minusbot YouTube TV Skill

## Overview

This skill includes a comprehensive logging system that provides:
- **Structured JSON logging** for production environments
- **Colored console output** for development
- **Environment-based configuration** for easy log level management

## Features

### Dual Format Support
- **JSON format** for production (machine-readable, easy to parse)
- **Colored format** for development (human-readable, easy to scan)

### Configurable Log Levels
- DEBUG
- INFO
- WARNING
- ERROR
- CRITICAL

## Usage

### Quick Start

```python
from logging_utils import setup_logging

logger = setup_logging("youtubetv")
logger.info("This is an info message")
logger.error("This is an error message")
```

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `LOG_LEVEL` | Set default log level | `LOG_LEVEL=DEBUG` |
| `JSON_LOGS` | Enable JSON output | `JSON_LOGS=1` |

### Production Example

```bash
# Enable JSON logging for production
export LOG_LEVEL=INFO
export JSON_LOGS=1

python wrapper.py '{"mode": "discover"}'
```

Output:
```json
{
  "timestamp": "2026-02-18T12:00:00.000000+00:00",
  "level": "INFO",
  "logger": "youtubetv",
  "message": "Scanning for devices (timeout=5s)...",
  "module": "wrapper",
  "function": "handle_discover",
  "line": 167
}
```

### Development Example

```bash
# Enable colored output for development
export LOG_LEVEL=DEBUG

python wrapper.py '{"mode": "discover"}'
```

Output:
```
2026-02-18 12:00:00 - youtubetv - INFO - Scanning for devices (timeout=5s)...
2026-02-18 12:00:01 - youtubetv - DEBUG - Device 'living_room' not in favorites. Scanning...
```

## Logging Levels in the Code

The logging system uses appropriate levels:

- **DEBUG**: Detailed information for troubleshooting
- **INFO**: Confirmation that things are working as expected
- **WARNING**: Indication of something unexpected
- **ERROR**: A more serious problem
- **CRITICAL**: A very serious error

## Integration with Skills

All YouTube TV skill scripts automatically use the shared logging system:

```python
from logging_utils import setup_logging

logger = setup_logging("youtubetv")
```

This provides:
- ✅ Consistent logging across all scripts
- ✅ Standardized output format
- ✅ Easy debugging and monitoring
- ✅ Production-ready structured logs

## Best Practices

1. **Use appropriate log levels**: Don't use DEBUG for normal operations
2. **Avoid f-strings in logs**: Use `%` formatting for better performance
3. **Include relevant context**: Log important variables for debugging
4. **Use structured data**: For JSON output, ensure data is properly formatted

## Migration Guide

Old code (using `print`):
```python
print("Scanning for devices (timeout={}s)...".format(timeout))
```

New code (using logger):
```python
logger.info("Scanning for devices (timeout=%ss)...", timeout)
```

Note: Use `%` formatting instead of f-strings for better performance.

## Troubleshooting

### No logs appearing
- Check `LOG_LEVEL` environment variable
- Ensure `setup_logging()` is called before logging

### Logs not in JSON format
- Set `JSON_LOGS=1` environment variable
- Verify the logger is configured correctly

### Library stubs missing
- Install type stubs: `pip install types-requests`
- Or ignore with: `--ignore-missing-imports`
