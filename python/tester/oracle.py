import asyncio
import os
import hashlib
import re

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
    normalized = normalized.replace(b"\r\n", b"\n").replace(b"\r", b"\n")
    return normalized


def get_structural_signature(query: str) -> str:
    sig = query.lower()
    sig = re.sub(r"\b\d+\b", "N", sig)
    sig = re.sub(r"'.*?'", "S", sig)
    sig = " ".join(sig.split())
    return sig


def stderr_matches_any(stderr: bytes, patterns: list[bytes]) -> bool:
    return any(p in stderr for p in patterns)


def is_expected_error(capture: TestCapture) -> bool:
    """True when the result looks like a normal, user-caused SQL error."""
    return stderr_matches_any(capture.stderr, EXPECTED_SQL_STDERR_PATTERNS)


def is_unconditional_bug(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, UNCONDITIONAL_BUG_PATTERNS)


def is_interesting_unilateral(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, INTERESTING_UNILATERAL_PATTERNS)


# shared across orcales
seen_signatures: set[bytes] = set()


async def oracle_worker(incoming: asyncio.Queue[TestCapture | None], oracle_path: str):
    # Could in theory also spawn multiple workers here, but 1 should be enough, especially since it should never crash
    oracle_worker = SQLiteWorker(oracle_path)
    os.makedirs("docker_out/crashes", exist_ok=True)

    while True:
        item = await incoming.get()
        if item is None:
            incoming.task_done()
            return

        bug_type: str | None = None
        notes: str = ""
        ref: TestCapture | None = None

        normalized_item_stderr = normalize_output(item.stderr)

        # hang, logic bug are often due to legit stuff like recursive CTE or random. do not care about different variation sof these
        if item.exit_code == 42:
            query = get_structural_signature(item.query)
        else:
            query = item.query
        # Create a signature that includes normalized stderr and a hash of the query for better deduplication
        query_hash = hashlib.md5(query.encode()).hexdigest().encode()
        signature = normalized_item_stderr + b"|" + query_hash
        if signature in seen_signatures:
            incoming.task_done()
            continue
        seen_signatures.add(signature)

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
                elif (
                    normalized_item_stderr.split(b"\n")[0]
                    != normalize_output(ref.stderr).split(b"\n")[0]
                ):
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
            # hang, logic bug are often due to legit stuff like recursive CTE or random. do not care about different variation sof these
            if bug_type == "LOGIC_BUG":
                query = get_structural_signature(item.query)
                query_hash = hashlib.md5(query.encode()).hexdigest().encode()
                signature = normalized_item_stderr + b"|" + query_hash
                if signature in seen_signatures:
                    incoming.task_done()
                    continue
                seen_signatures.add(signature)

            filename = f"docker_out/crashes/bug_{query_hash.hex()}_{bug_type}.txt"
            if os.path.exists(filename):
                incoming.task_done()
                continue
            print(f"[!] {bug_type} — saving to {filename}", flush=True)

            ref_block = ""
            if ref is not None:
                ref_block = f"\n--- Reference (/usr/bin/sqlite3-3.39.4) ---\n{ref}"
            try:
                with open(filename, "x", encoding="utf-8") as f:
                    _ = f.write(
                        f"{bug_type} REPORT\n\
                        Notes: {notes}\n\n\
                        Query:\n{item.query}\n\
                        {ref_block}\n\
                        --- Found (sqlite3_guarded) ---\n{item}"
                    )
            except FileExistsError:
                # Another worker finished the same bug just before us
                pass

        incoming.task_done()
