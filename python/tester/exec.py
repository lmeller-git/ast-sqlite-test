import asyncio
import re

from lib_sf.lib_sf import RejectionReason, TestOutcome, TestableEntry
from lib_sf import engine
from tester.persistent_worker import SQLiteWorker, TestCapture


async def run_single_mutation(
    entry: TestableEntry,
    ipc_queue: engine.IPCTokenQueue,
    mutation_engine: engine.Engine,
    oracle_queue: asyncio.PriorityQueue[tuple[int, TestCapture | None]],
    workers: dict[int, SQLiteWorker],
    test_path: str,
):
    backoff = 0.01
    token = ipc_queue.pop()
    while token is None:
        await asyncio.sleep(backoff)
        backoff = min(backoff * 2, 0.5)
        token = ipc_queue.pop()

    try:
        token_id = token.id()
        if token_id not in workers:
            workers[token_id] = SQLiteWorker(
                test_path, {"FUZZER_SHMEM_PATH": token.as_env(), "ASAN_OPTIONS": "detect_leaks=0"}
            )
        worker = workers[token_id]
        capture = await worker.execute(entry.to_sql_string())
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

        if is_crash or is_hang:
            entry.fire_hooks(TestOutcome.rejected(RejectionReason.crash()))
            mutation_engine.return_token(token)
        elif is_syntax_err:
            entry.fire_hooks(TestOutcome.rejected(RejectionReason.invalid_syntax()))
            mutation_engine.return_token(token)
        else:
            mutation_engine.commit_test_result(entry, engine.TestResult(capture.exec_time, token))

        if is_crash:
            capture.is_hang_or_crash = "CRASH"

        priority = -capture.exec_time
        if is_crash:
            priority //= 10
        if is_hang:
            priority //= 2

        await oracle_queue.put((-priority, capture))

    except Exception:
        entry.fire_hooks(TestOutcome.rejected(RejectionReason.invalid_syntax()))
        mutation_engine.return_token(token)


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
