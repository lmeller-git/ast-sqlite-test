"""
SQLite Executor Oracle - Executes SQL queries and captures results for testing.
"""

import sqlite3
import hashlib
import time
from dataclasses import dataclass
from enum import Enum
from typing import Optional, List, Tuple
from contextlib import contextmanager


class ExecutionStatus(Enum):
    """Status of SQL query execution."""

    SUCCESS = "success"
    SYNTAX_ERROR = "syntax_error"
    RUNTIME_ERROR = "runtime_error"
    TIMEOUT = "timeout"
    CRASH = "crash"


@dataclass
class ExecutionResult:
    """Result of executing a SQL query."""

    status: ExecutionStatus
    result_hash: str
    execution_time: float
    error_message: Optional[str] = None
    error_type: Optional[str] = None
    output_rows: int = 0

    def is_success(self) -> bool:
        """Check if query executed successfully."""
        return self.status == ExecutionStatus.SUCCESS

    def is_error(self) -> bool:
        """Check if query produced an error."""
        return self.status in [
            ExecutionStatus.SYNTAX_ERROR,
            ExecutionStatus.RUNTIME_ERROR,
            ExecutionStatus.CRASH,
        ]

    def __hash__(self):
        """Hash based on result, allowing comparison of unique outputs."""
        return hash((self.result_hash, self.error_type))


class SqliteExecutor:
    """
    Executes SQL queries against an in-memory SQLite database.
    Handles errors, timeouts, and result normalization.
    """

    def __init__(self, timeout: float = 5.0, in_memory: bool = True):
        """
        Initialize executor.

        Args:
            timeout: Query timeout in seconds
            in_memory: Use in-memory database if True, temporary file otherwise
        """
        self.timeout = timeout
        self.in_memory = in_memory
        self.db_path = ":memory:" if in_memory else None

    @contextmanager
    def _get_connection(self):
        """Context manager for database connections."""
        conn = sqlite3.connect(self.db_path, timeout=self.timeout)
        try:
            yield conn
        finally:
            conn.close()

    def execute(self, query: str) -> ExecutionResult:
        """
        Execute a SQL query and return result.

        Args:
            query: SQL query to execute (can be string or list of strings)

        Returns:
            ExecutionResult with status, output hash, and metadata
        """
        start_time = time.time()

        # Handle if restore_ast returns a list instead of a string
        # TODO - Ideally restore_ast should always return a string, but I wrote this as a temporary fix - Kevin
        if isinstance(query, list):
            query = ";".join(query)

        try:
            with self._get_connection() as conn:
                cursor = conn.cursor()

                # Execute the query (handle both single and multiple statements)
                try:
                    cursor.executescript(query)
                except sqlite3.OperationalError as e:
                    # Runtime error (table doesn't exist, etc.)
                    elapsed = time.time() - start_time
                    return ExecutionResult(
                        status=ExecutionStatus.RUNTIME_ERROR,
                        result_hash=self._hash_error(str(e)),
                        execution_time=elapsed,
                        error_message=str(e),
                        error_type="OperationalError",
                        output_rows=0,
                    )
                except sqlite3.ProgrammingError as e:
                    # Syntax error
                    elapsed = time.time() - start_time
                    return ExecutionResult(
                        status=ExecutionStatus.SYNTAX_ERROR,
                        result_hash=self._hash_error(str(e)),
                        execution_time=elapsed,
                        error_message=str(e),
                        error_type="ProgrammingError",
                        output_rows=0,
                    )
                except Exception as e:
                    # Unexpected error
                    elapsed = time.time() - start_time
                    return ExecutionResult(
                        status=ExecutionStatus.CRASH,
                        result_hash=self._hash_error(str(e)),
                        execution_time=elapsed,
                        error_message=str(e),
                        error_type=type(e).__name__,
                        output_rows=0,
                    )

                # Fetch results and compute hash from last query
                try:
                    rows = cursor.fetchall()
                    result_hash = self._hash_rows(rows)
                    elapsed = time.time() - start_time

                    return ExecutionResult(
                        status=ExecutionStatus.SUCCESS,
                        result_hash=result_hash,
                        execution_time=elapsed,
                        output_rows=len(rows),
                    )
                except Exception as e:
                    elapsed = time.time() - start_time
                    return ExecutionResult(
                        status=ExecutionStatus.CRASH,
                        result_hash=self._hash_error(str(e)),
                        execution_time=elapsed,
                        error_message=str(e),
                        error_type=type(e).__name__,
                        output_rows=0,
                    )

        except Exception as e:
            # Connection error
            elapsed = time.time() - start_time
            return ExecutionResult(
                status=ExecutionStatus.CRASH,
                result_hash=self._hash_error(str(e)),
                execution_time=elapsed,
                error_message=str(e),
                error_type=type(e).__name__,
                output_rows=0,
            )

    @staticmethod
    def _hash_rows(rows: List[Tuple]) -> str:
        """
        Hash query results for comparison.
        Normalizes floating-point precision issues.
        """
        # Normalize and serialize rows
        normalized = []
        for row in rows:
            normalized_row = []
            for val in row:
                # Handle floating-point precision
                if isinstance(val, float):
                    normalized_row.append(round(val, 10))
                else:
                    normalized_row.append(val)
            normalized.append(tuple(normalized_row))

        # Create hash
        hash_input = str(sorted(normalized)).encode("utf-8")
        return hashlib.sha256(hash_input).hexdigest()[:16]

    @staticmethod
    def _hash_error(error_msg: str) -> str:
        """Hash error message for duplicate error detection."""
        # Normalize error message (remove line numbers, etc.)
        normalized = "".join(c for c in error_msg if c.isalpha() or c.isspace())
        hash_input = normalized.encode("utf-8")
        return hashlib.sha256(hash_input).hexdigest()[:16]


class DuplicateDetector:
    """Detects duplicate and redundant test cases."""

    def __init__(self):
        self.seen_hashes: set = set()
        self.error_hashes: dict = {}  # error_category -> count

    def is_duplicate(self, result: ExecutionResult) -> bool:
        """Check if result is a duplicate of previously seen result."""
        result_key = (result.result_hash, result.error_type)

        if result_key in self.seen_hashes:
            return True

        self.seen_hashes.add(result_key)
        return False

    def record_error(self, result: ExecutionResult) -> None:
        """Record error for deduplication."""
        if result.error_type:
            self.error_hashes[result.error_type] = self.error_hashes.get(result.error_type, 0) + 1

    def get_stats(self) -> dict:
        """Get deduplication statistics."""
        return {"unique_results": len(self.seen_hashes), "error_types": self.error_hashes}
