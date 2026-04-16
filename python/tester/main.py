from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
from asyncio import PriorityQueue, Task
from lib_sf.lib_sf import RawEntry
from dataclasses import dataclass, field
import re
import os
import time

global CRASHES_FOUND


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
    oracle_queue: PriorityQueue[tuple[int, TestCapture]],
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


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: PriorityQueue[tuple[int, TestCapture]],
):
    active_tasks: set[Task[None]] = set()
    CONCURRENCY_LIMIT = 8
    TASK_QUEUE_LIMIT = CONCURRENCY_LIMIT * 2

    while True:
        if len(active_tasks) < TASK_QUEUE_LIMIT:
            batch = mutation_engine.mutate_batch(TASK_QUEUE_LIMIT - len(active_tasks))
            for entry in batch.into_members():
                task = asyncio.create_task(
                    run_single_mutation(entry, ipc_queue, mutation_engine, oracle_queue)
                )
                active_tasks.add(task)
        else:
            mutation_engine.gc()

        if not active_tasks:
            continue

        _done, active_tasks = await asyncio.wait(active_tasks, return_when=asyncio.FIRST_COMPLETED)


async def oracle(incoming: PriorityQueue[tuple[int, TestCapture]]):
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


async def main(args: Namespace):
    max_edges = await init()
    print("found ", max_edges, " max_edges")
    ipc_queue = engine.IPCTokenQueue(8, max_edges)
    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.weighted_random(),
        [engine.StrategyBuilder.table_guard()],
        ipc_queue,
        42,
    )

    mutation_engine.populate(
        [
            engine.SeedGeneratorBuilder.dir_reader(args.seeds)
            if args.seeds is not None
            else engine.SeedGeneratorBuilder.literal("CREATE TABLE B; SELECT a FROM B"),
            engine.SeedGeneratorBuilder.literal(
                "\
                CREATE TABLE t0(c0 REAL UNIQUE);\
                INSERT INTO t0(c0) VALUES (3175546974276630385);\
                SELECT 3175546974276630385 < c0 FROM t0;\
                SELECT 1 FROM t0 WHERE 3175546974276630385 < c0;\
                "
            ),
        ]
    )

    oracle_queue = PriorityQueue(1024)

    oracle_task = asyncio.create_task(oracle(oracle_queue))

    # TODO: force add guarded queries back to engine or skip this entirely

    mutation_engine.clear_strategies()
    [
        mutation_engine.add_strategy(strat)
        for strat in [
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.splice_in(), 0.6),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_scrambler(), 0.3),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.op_flip(), 0.5),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.num_bounds(), 0.5),
        ]
    ]

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())

    tasks = [
        run_single_mutation(entry.clone_raw(), ipc_queue, mutation_engine, oracle_queue)
        for entry in snapshot
    ]

    r = await asyncio . gather(*tasks)

    print(f"Done executing {r.__len__()} setup queries", flush=True)

    mutation_engine.gc()

    print("init done, entering loop")

    _ = await asyncio.gather(
        fuzzing_loop(mutation_engine, ipc_queue, oracle_queue), oracle_task
    )

    print("after loop:\n")

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    args = parser.parse_args()
    asyncio.run(main(args))
