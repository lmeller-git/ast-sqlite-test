from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
from asyncio import PriorityQueue
from lib_sf.lib_sf import RawEntry
from dataclasses import dataclass, field
import re
import os
import time


@dataclass(order=True)
class TestCapture:
    stdout: bytes = field(compare=False)
    stderr: bytes = field(compare=False)
    exit_code: int | None = field(compare=False)
    query: str = field(compare=False)
    exec_time: int

    def __format__(self, format_spec: str) -> str:
        return f"TestCapture {{\nstdout: {self.stdout.decode()}\nstdserr: {self.stderr.decode()}\nexit_code: {self.exit_code}\nexec_time:{self.exec_time}\nquery: {self.query}\n}}"


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: PriorityQueue[tuple[int, TestCapture]],
):
    async def run_single_mutation(entry: RawEntry):
        # TODO add exponential backoff
        token = ipc_queue.pop()
        while token is None:
            await asyncio.sleep(0.01)
            token = ipc_queue.pop()

        result = await execute_query(
            "./sqlite3/sqlite3_guarded",
            entry.to_sql_string(),
            {"FUZZER_SHMEM_PATH": token.as_env()},
        )

        mutation_engine.commit_test_result(entry, engine.TestResult(result.exec_time, token))

        # TODO prio by time + ecit_code + stderr
        priority = result.exec_time
        await oracle_queue.put((-priority, result))

    while True:
        batch = mutation_engine.mutate_batch(8)
        members = batch.into_members()

        if not members:
            await asyncio.sleep(0.1)
            continue

        tasks = [run_single_mutation(entry) for entry in members]
        _ = await asyncio.gather(*tasks)


async def oracle(mutation_engine: engine.Engine, incoming: PriorityQueue[tuple[int, TestCapture]]):
    while True:
        _, next_item = await incoming.get()

        expected = await execute_query("/usr/bin/sqlite3-3.39.4", next_item.query)

        if (
            expected.stderr != next_item.stderr
            or expected.stdout != next_item.stdout
            or expected.exit_code != next_item.exit_code
        ):
            print(
                f"found bug in query: {next_item.query}\nExpected: {expected}\nFound: {next_item}"
            )

        incoming.task_done()


async def execute_query(cmd: str, query: str, env: dict[str, str] | None = None) -> TestCapture:
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

    stdout, stderr = await proc.communicate(input=query.encode())

    exec_time = time.perf_counter_ns() - start_time

    return TestCapture(stdout, stderr, proc.returncode, query, exec_time)


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

    # TODO: force add guarded queries back to engine or skip this entirely

    mutation_engine.clear_strategies()
    [
        mutation_engine.add_strategy(strat)
        for strat in [engine.StrategyBuilder.splice_in(), engine.StrategyBuilder.table_scrambler()]
    ]

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())

    _ = await asyncio.gather(
        fuzzing_loop(mutation_engine, ipc_queue, oracle_queue),
        oracle(mutation_engine, oracle_queue),
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
