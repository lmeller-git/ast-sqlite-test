from dataclasses import dataclass, field
import asyncio
import re
import os
import time

from lib_sf.lib_sf import RawEntry
from lib_sf import engine


@dataclass(order=True)
class TestCapture:
    stdout: bytes = field(compare=False)
    stderr: bytes = field(compare=False)
    exit_code: int | None = field(compare=False)
    query: str = field(compare=False)
    exec_time: int
    is_hang_or_crash: None | str = field(compare=False)

    def __format__(self, format_spec: str) -> str:
        return f"TestCapture {{\nstdout: {self.stdout.decode()}\nstdserr: {self.stderr.decode()}\nexit_code: {self.exit_code}\nexec_time:{self.exec_time}\nquery: {self.query}\n}}"


async def run_single_mutation(
    entry: RawEntry,
    ipc_queue: engine.IPCTokenQueue,
    mutation_engine: engine.Engine,
    oracle_queue: asyncio.PriorityQueue[tuple[int, TestCapture]],
):
    # TODO add exponential backoff
    token = ipc_queue.pop()
    while token is None:
        await asyncio.sleep(0.01)
        token = ipc_queue.pop()

    result = await execute_query(
        "./sqlite3/sqlite3_guarded",
        entry.to_sql_string(),
        {"FUZZER_SHMEM_PATH": token.as_env(), "ASAN_OPTIONS": "detect_leaks=0"},
    )

    is_hang = result.exit_code is not None and result.exit_code == 42
    is_crash = (
        (not is_hang and result.exit_code is not None and result.exit_code != 0)
        or b"AddressSanitizer" in result.stderr
        or b"Assertion" in result.stderr
    )

    if not is_crash and not is_hang:
        mutation_engine.commit_test_result(entry, engine.TestResult(result.exec_time, token))
    else:
        mutation_engine.return_token(token)

    if is_crash:
        result.is_hang_or_crash = "CRASH"

    priority = -result.exec_time
    if is_crash:
        priority //= 10
    if is_hang:
        priority //= 2

    await oracle_queue.put((-priority, result))


async def execute_query(
    cmd: str, query: str, env: dict[str, str] | None = None, timeout_sec: float = 1.5
) -> TestCapture:
    full_env = os.environ.copy()
    if env is not None:
        full_env.update(env)

    start_time = time.perf_counter_ns()

    proc = await asyncio.create_subprocess_exec(
        cmd,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=full_env,
    )

    try:
        stdout, stderr = await asyncio.wait_for(
            proc.communicate(input=query.encode()), timeout=timeout_sec
        )
        exec_time = time.perf_counter_ns() - start_time
        return TestCapture(stdout, stderr, proc.returncode, query, exec_time, None)

    except asyncio.TimeoutError:
        try:
            proc.kill()
            _ = await proc.wait()
        except ProcessLookupError:
            print("couldnt kill timed out process", flush=True)
            pass

        exec_time = time.perf_counter_ns() - start_time

        return TestCapture(
            stdout=b"",
            stderr=b"EXECUTION TIMEOUT EXCEEDED",
            exit_code=42,
            query=query,
            exec_time=exec_time,
            is_hang_or_crash="HANG",
        )


async def init() -> int:
    res = await execute_query("./sqlite3/sqlite3_guarded", ".quit", {"FUZZER_INIT": "1"})
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
