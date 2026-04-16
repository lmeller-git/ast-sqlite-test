import asyncio
import os

from tester.exec import TestCapture, execute_query


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
]


def stderr_matches_any(stderr: bytes, patterns: list[bytes]) -> bool:
    return any(p in stderr for p in patterns)


def is_expected_error(capture: TestCapture) -> bool:
    """True when the result looks like a normal, user-caused SQL error."""
    return stderr_matches_any(capture.stderr, EXPECTED_SQL_STDERR_PATTERNS)


def is_unconditional_bug(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, UNCONDITIONAL_BUG_PATTERNS)


def is_interesting_unilateral(capture: TestCapture) -> bool:
    return stderr_matches_any(capture.stderr, INTERESTING_UNILATERAL_PATTERNS)


async def oracle(incoming: asyncio.PriorityQueue[tuple[int, TestCapture]]):
    crash_counter = 0
    seen_signatures: set[bytes] = set()
    os.makedirs("crashes", exist_ok=True)

    while True:
        _, item = await incoming.get()

        bug_type: str | None = None
        notes: str = ""
        ref: TestCapture | None = None

        if is_unconditional_bug(item):
            bug_type = "MEMSAFETY"

        # annotated by exec
        elif item.exit_code == 42:
            bug_type = "HANG"

        elif is_interesting_unilateral(item):
            ref = await execute_query("/usr/bin/sqlite3-3.39.4", item.query)
            if not is_interesting_unilateral(ref):
                bug_type = "INTERNAL_ERROR"
                notes = "Reference did not produce an internal/corrupt error."

        elif item.exit_code != 0:
            ref = await execute_query("/usr/bin/sqlite3-3.39.4", item.query)

            if ref.exit_code == 0:
                bug_type = "CRASH_OR_ERROR"
                notes = f"Reference exited 0; target exited {item.exit_code}."
            else:
                if is_expected_error(item) and is_expected_error(ref):
                    pass
                elif item.stderr.split(b"\n")[0] != ref.stderr.split(b"\n")[0]:
                    bug_type = "DIVERGENCE"
                    notes = "Both errored, but with different messages."

        elif item.exit_code == 0:
            ref = await execute_query("/usr/bin/sqlite3-3.39.4", item.query)

            if ref.exit_code != 0:
                bug_type = "DIVERGENCE"
                notes = f"Target exited 0 but reference exited {ref.exit_code}."
            elif ref.stdout != item.stdout:
                bug_type = "LOGIC_BUG"
                notes = "Same exit code (0) but stdout differs."

        if bug_type is not None:
            if item.stderr in seen_signatures:
                incoming.task_done()
                continue
            seen_signatures.add(item.stderr)

            filename = f"crashes/bug_{crash_counter:04d}_{bug_type}.txt"
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


async def oracle__(incoming: asyncio.PriorityQueue[tuple[int, TestCapture]]):
    crash_counter = 0
    os.makedirs("crashes", exist_ok=True)

    while True:
        _, next_item = await incoming.get()

        if next_item.is_hang_or_crash is not None and next_item.is_hang_or_crash == "CRASH":
            if b"Parse error" in next_item.stderr:
                continue
            filename = f"crashes/bug_{crash_counter}.txt"
            print(f"CRASH FOUND! Saving report to {filename}", flush=True)

            with open(filename, "w", encoding="utf-8") as f:
                _ = f.write(
                    f"CRASH REPORT\n\
                \nQuery: \n{next_item.query}\n\
                \n--- Found (sqlite3_guarded) ---\n\
                {next_item}"
                )

            crash_counter += 1
            incoming.task_done()
            continue

        expected = await execute_query("/usr/bin/sqlite3-3.39.4", next_item.query)

        bug_type = None

        if next_item.exit_code == 42:
            bug_type = "HANG"

        elif (
            (next_item.exit_code is not None and next_item.exit_code < 0)
            or b"AddressSanitizer" in next_item.stderr
            or b"Assertion" in next_item.stderr
        ):
            bug_type = "CRASH"

        elif (
            expected.exit_code != 0
            and next_item.exit_code != 0
            and expected.exit_code == next_item.exit_code
        ):
            pass

        elif expected.exit_code == 0 and next_item.exit_code == 0:
            if expected.stdout != next_item.stdout:
                bug_type = "LOGIC_BUG"

        else:
            if b"no such module" in expected.stderr or b"no such module" in next_item.stderr:
                pass
            else:
                bug_type = "DIVERGENCE"

        if bug_type is not None:
            filename = f"crashes/bug_{crash_counter}.txt"
            print(f"{bug_type} FOUND! Saving report to {filename}", flush=True)

            with open(filename, "w", encoding="utf-8") as f:
                _ = f.write(
                    f"{bug_type} REPORT\n\
                \nQuery: \n{next_item.query}\n\
                \n--- Expected (/usr/bin/sqlite3-3.39.4) ---\n\
                {expected}\
                \n--- Found (sqlite3_guarded) ---\n\
                {next_item}"
                )

            crash_counter += 1

        incoming.task_done()
