import asyncio
import os
import hashlib
from tester.persistent_worker import SQLiteWorker, TestCapture


# list of interesting patterns llm generated

EXPECTED_SQL_STDERR_PATTERNS: list[bytes] = [
    b"Parse error",
    b"syntax error",
    b"no such table",
    b"no such column",
    b"no such function",
    b"no such module",  # virtual-table extension not loaded
    b"no such index",
    b"ambiguous column name",
    b"table already exists",
    b"duplicate column name",
    b"cannot open value of type",
    b"no tables specified",
    b"UNIQUE constraint failed",
    b"NOT NULL constraint failed",
    b"CHECK constraint failed",
    b"FOREIGN KEY constraint failed",
    b"too many levels of trigger",
    b"string or blob too big",  # SQLITE_TOOBIG — valid user error
    b"too many columns",
    b"too many terms in compound SELECT",
    b"too many attached databases",
    b"interrupted",  # SQLITE_INTERRUPT — valid in concurrent tests
    b"unable to open database",  # SQLITE_CANTOPEN — env issue, not a bug
    b"disk I/O error",  # SQLITE_IOERR — env issue
    b"database is locked",  # SQLITE_LOCKED/BUSY — env issue
    b"out of memory",  # SQLITE_NOMEM — env issue
    b"RuntimeError",  # Python-level error from the CLI harness
    b"Error: ",  # Generic CLI-level error prefix (catches most SQLITE_ERROR=1)
]

# These are ALWAYS bugs — memory safety violations.
# Don't bother running the reference; save immediately.
UNCONDITIONAL_BUG_PATTERNS: list[bytes] = [
    b"AddressSanitizer",
    b"LeakSanitizer",
    b"Assertion failed",
    b"SIGABRT",
    b"SIGSEGV",
    b"heap-buffer-overflow",
    b"stack-buffer-overflow",
    b"use-after-free",
    b"double-free",
]

# These are interesting even if the reference also errors,
# because they indicate *internal* SQLite corruption/misuse:
INTERESTING_UNILATERAL_PATTERNS: list[bytes] = [
    b"internal error",  # maps to SQLITE_INTERNAL (2)
    b"database disk image is malformed",  # SQLITE_CORRUPT (11) — suspicious in fuzz
    b"misuse of aggregate",
    b"misuse of window function",
    b"database corruption",
    b"index corruption",
]


def normalize_output(output: bytes) -> bytes:
    """Normalize output for comparison: strip whitespace, lowercase, normalize newlines."""
    # Strip leading/trailing whitespace
    normalized = output.strip()
    # Lowercase for case-insensitive comparison
    normalized = normalized.lower()
    # Normalize newlines to \n
    normalized = normalized.replace(b'\r\n', b'\n').replace(b'\r', b'\n')
    return normalized


def stderr_matches_any(stderr: bytes, patterns: list[bytes]) -> bool:
    return any(p in stderr for p in patterns)


def is_expected_error(capture: TestCapture) -> bool:
    """True when the result looks like a normal, user-caused SQL error."""
    return stderr_matches_any(capture.stderr, EXPECTED_SQL_STDERR_PATTERNS)


def is_unconditional_bug(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, UNCONDITIONAL_BUG_PATTERNS)


def is_interesting_unilateral(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, INTERESTING_UNILATERAL_PATTERNS)


async def oracle(incoming: asyncio.PriorityQueue[tuple[int, TestCapture | None]], oracle_path: str):
    crash_counter = 0
    seen_signatures: set[bytes] = set()
    # Could in theory also spawn multiple workers here, but 1 should be enough, especially since it should never crash
    oracle_worker = SQLiteWorker(oracle_path)
    os.makedirs("docker_out/crashes", exist_ok=True)

    while True:
        _, item = await incoming.get()
        if item is None:
            return

        bug_type: str | None = None
        notes: str = ""
        ref: TestCapture | None = None
        
        normalized_item_stderr = normalize_output(item.stderr)

        if is_unconditional_bug(item):
            bug_type = "MEMSAFETY"

        # annotated by exec
        elif item.exit_code == 42:
            bug_type = "HANG"

        elif is_interesting_unilateral(item):
            ref = await oracle_worker.execute(item.query)
            if not is_interesting_unilateral(ref):
                bug_type = "INTERNAL_ERROR"
                notes = "Reference did not produce an internal/corrupt error."

        elif item.exit_code != 0:
            ref = await oracle_worker.execute(item.query)

            if ref.exit_code == 0:
                bug_type = "CRASH_OR_ERROR"
                notes = f"Reference exited 0; target exited {item.exit_code}."
            else:
                if is_expected_error(item) and is_expected_error(ref):
                    pass
                elif normalized_item_stderr.split(b"\n")[0] != normalize_output(ref.stderr).split(b"\n")[0]:
                    bug_type = "DIVERGENCE"
                    notes = "Both errored, but with different messages (after normalization)."

        elif item.exit_code == 0:
            ref = await oracle_worker.execute(item.query)

            if ref.exit_code != 0:
                bug_type = "DIVERGENCE"
                notes = f"Target exited 0 but reference exited {ref.exit_code}."
            elif normalize_output(ref.stdout) != normalize_output(item.stdout):
                bug_type = "LOGIC_BUG"
                notes = "Same exit code (0) but stdout differs (after normalization)."
            elif ref.stdout != item.stdout:
                bug_type = "LOGIC_BUG"
                notes = "Same exit code (0) but stdout differs."

        if bug_type is not None:
            # Create a signature that includes normalized stderr and a hash of the query for better deduplication
            query_hash = hashlib.md5(item.query.encode()).hexdigest().encode()
            signature = normalized_item_stderr + b"|" + query_hash
            if signature in seen_signatures:
                incoming.task_done()
                continue
            seen_signatures.add(signature)

            filename = f"docker_out/crashes/bug_{crash_counter:04d}_{bug_type}.txt"
            print(f"[!] {bug_type} — saving to {filename}", flush=True)

            ref_block = ""
            if ref is not None:
                ref_block = f"\n--- Reference (/usr/bin/sqlite3-3.39.4) ---\n{ref}"

            with open(filename, "w", encoding="utf-8") as f:
                _ = f.write(
                    f"{bug_type} REPORT\n\
                    Notes: {notes}\n\n\
                    Query:\n{item.query}\n\
                    {ref_block}\n\
                    --- Found (sqlite3_guarded) ---\n{item}"
                )

            crash_counter += 1

        incoming.task_done()

