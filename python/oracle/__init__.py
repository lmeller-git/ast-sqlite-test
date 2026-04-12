"""
Oracle module for SQL fuzzing - provides execution and evaluation capabilities.
"""

from .executor import (
    ExecutionStatus,
    ExecutionResult,
    SqliteExecutor,
    DuplicateDetector,
)

__all__ = [
    "ExecutionStatus",
    "ExecutionResult",
    "SqliteExecutor",
    "DuplicateDetector",
]
