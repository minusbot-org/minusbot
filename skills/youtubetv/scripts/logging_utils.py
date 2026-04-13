"""
logging_utils.py — Shared logging utilities for Minusbot
========================================================

Provides consistent logging configuration across all skills with:
- Console output with colored levels
- Structured JSON logging for production
- Configurable verbosity via environment variable
"""

import logging
import os
import sys
import json
from datetime import datetime, timezone
from typing import ClassVar


class JSONFormatter(logging.Formatter):
    """Format log records as JSON for structured logging.

    Output format includes:
    - timestamp: ISO 8601 formatted UTC time
    - level: Log level name
    - logger: Logger name
    - message: Log message
    - module: Source module name
    - function: Source function name
    - line: Source line number
    - exception: Exception info if present
    """

    def format(self, record: logging.LogRecord) -> str:
        log_data = {
            "timestamp": datetime.now(tz=timezone.utc).isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
            "module": record.module,
            "function": record.funcName,
            "line": record.lineno,
        }

        if record.exc_info:
            log_data["exception"] = self.formatException(record.exc_info)

        return json.dumps(log_data)


class ColoredFormatter(logging.Formatter):
    """Format log records with colored levels for console output.

    Uses ANSI color codes for different log levels:
    - DEBUG: Cyan
    - INFO: Green
    - WARNING: Yellow
    - ERROR: Red
    - CRITICAL: Bold Red
    """

    COLORS: ClassVar[dict[str, str]] = {
        "DEBUG": "\033[36m",
        "INFO": "\033[32m",
        "WARNING": "\033[33m",
        "ERROR": "\033[31m",
        "CRITICAL": "\033[31;1m",
    }
    RESET = "\033[0m"

    def format(self, record: logging.LogRecord) -> str:
        level_color = self.COLORS.get(record.levelname, "")
        message = super().format(record)
        return f"{level_color}{message}{self.RESET}"


def setup_logging(
    name: str = "minusbot",
    level: str | None = None,
    json_output: bool = False,
) -> logging.Logger:
    """Set up logging configuration for a skill.

    Args:
        name: Logger name (usually the skill name)
        level: Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
        json_output: If True, output JSON format for production

    Returns:
        Configured logger instance

    Environment variables:
        LOG_LEVEL: Default log level (default: INFO)
        JSON_LOGS: Set to 1/true/yes for JSON output
    """
    if level is None:
        level = os.environ.get("LOG_LEVEL", "INFO").upper()

    log_level = getattr(logging, level, logging.INFO)

    logger = logging.getLogger(name)
    logger.setLevel(log_level)

    if logger.handlers:
        return logger

    handler = logging.StreamHandler(sys.stdout)
    handler.setLevel(log_level)

    formatter: logging.Formatter = JSONFormatter("{message}", style="{")
    if not json_output and os.environ.get("JSON_LOGS", "0").lower() not in (
        "1",
        "true",
        "yes",
    ):
        formatter = ColoredFormatter(
            "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
        )

    handler.setFormatter(formatter)
    logger.addHandler(handler)

    logger.propagate = False

    return logger


def get_logger(name: str) -> logging.Logger:
    """Get a logger instance for a module.

    Args:
        name: Logger name (usually __name__)

    Returns:
        Logger instance
    """
    return logging.getLogger(name)
