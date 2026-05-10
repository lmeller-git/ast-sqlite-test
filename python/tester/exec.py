import asyncio
import re

from lib_sf.lib_sf import RejectionReason, TestOutcome, TestableEntry
from lib_sf import engine
from tester.persistent_worker import SQLiteWorker, TestCapture

CONCURRENCY_LIMIT = 16

async def run_single_mutation(
    entry: TestableEntry,
    ipc_queue: engine.IPCTokenQueue,
    mutation_engine: engine.Engine,
    oracle_queue: asyncio.Queue[TestCapture | None],
    workers: dict[int, SQLiteWorker],
    test_path: str,
):
    token = await ipc_queue.recv()

    if token is None:
        return

    try:
        token_id = token.id()
        if token_id not in workers:
            workers[token_id] = SQLiteWorker(
                test_path, {"FUZZER_SHMEM_PATH": token.as_env(), "ASAN_OPTIONS": "detect_leaks=0"}
            )
        worker = workers[token_id]
        sql_str = entry.to_sql_string()
        capture = await worker.execute(sql_str, 1. + 0.1 * len(sql_str) / 1000)
        is_hang = (
            capture.exit_code is not None
            and capture.exit_code == 42
            or capture.is_hang_or_crash == "HANG"
        )
        is_crash = (
            (not is_hang and capture.exit_code is not None and capture.exit_code != 0)
            or b"AddressSanitizer" in capture.stderr
            or b"Assertion" in capture.stderr
            or capture.is_hang_or_crash == "CRASH"
        )

        is_syntax_err = b"syntax error" in capture.stderr or b"Parse error" in capture.stderr

        test_result = engine.TestResult(capture.exec_time, len(capture.query), token)

        # may want to return crashes and syntax errors also. They will quicly be moved to cold cache anyway, as they likely will not create good children
        # would need to acocunt for this in mab stats though
        if is_crash:
            entry.fire_hooks(TestOutcome.rejected(RejectionReason.crash()), test_result)
            # ipc_queue.send(test_result.token)
        elif is_hang:
            entry.fire_hooks(TestOutcome.rejected(RejectionReason.timeout()), test_result)
            # ipc_queue.send(test_result.token)
        elif is_syntax_err:
            entry.fire_hooks(TestOutcome.rejected(RejectionReason.invalid_syntax()), test_result)
            # ipc_queue.send(test_result.token)
        # else:
        mutation_engine.commit_test_result( entry, test_result)

        if is_crash:
            capture.is_hang_or_crash = "CRASH"

        await oracle_queue.put(capture)

    except Exception:
        test_result = engine.TestResult(0, 0, token)
        entry.fire_hooks(TestOutcome.rejected(RejectionReason.invalid_syntax()), test_result)
        ipc_queue.send(test_result.token)


async def init(test_path: str) -> int:
    worker = SQLiteWorker(test_path, {"FUZZER_INIT": "1"})

    res = await worker.execute(".quit")
    await worker.close()

    output = res.stdout.decode()

    match = re.search(r"FUZZER_INIT: max edges = (\d+)", output)
    if not match:
        raise RuntimeError(
            f"Failed to find max edges in output.\n \
            Return Code: {res.exit_code}\n \
            Stdout: '{output}'\n \
            Stderr: '{res.stderr.decode()}'"
        )

    return int(match.group(1))
