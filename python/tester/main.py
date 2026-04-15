from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
from asyncio import PriorityQueue
from lib_sf.lib_sf import RawEntry
from dataclasses import dataclass
import re
import os


@dataclass
class TestCapture:
    stdout: bytes
    stderr: bytes
    exit: int | None
    exec_time: int


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: PriorityQueue[tuple[int, TestCapture]],
):
    async def run_single_mutation(entry: RawEntry):
        # TODO wait
        token = ipc_queue.pop()
        if token is None:
            return

        result = await execute_query(
            "./sqlite3/sqlite3_guarded", entry.to_sql_string(), {"token_env": token.as_env()}
        )

        mutation_engine.commit_test_result(entry, engine.TestResult(result.exec_time, token))

        priority = 1
        await oracle_queue.put((-priority, result))

    while True:
        batch = mutation_engine.mutate_batch(8)
        tasks = [run_single_mutation(entry) for entry in batch.into_members()]
        _ = await asyncio.gather(*tasks)
        break


async def oracle(mutation_engine: engine.Engine, incoming: PriorityQueue[tuple[int, TestCapture]]):
    return
    while True:
        _, next_item = await incoming.get()

        expected = await execute_query(
            "sqlite3 ref", "TODO: should likely put the entry into the TestResult"
        )
        if expected != next_item:
            print("found bug")

        incoming.task_done()
        break


async def execute_query(cmd: str, query: str, env: dict[str, str] | None = None) -> TestCapture:
    # TODO spawn the tasks (maybe on a sparate thread)
    full_env = os.environ.copy()
    if env is not None:
        full_env.update(env)

    proc = await asyncio.create_subprocess_exec(
        cmd,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=full_env,
    )

    stdout, stderr = await proc.communicate(input=query.encode())
    return TestCapture(stdout, stderr, proc.returncode, 0)


async def init() -> int:
    res = await execute_query("./sqlite3/sqlite3_guarded", ".quit", {"FUZZER_INIT": "1"})
    output = res.stdout.decode()

    match = re.search(r"FUZZER_INIT: max edges = (\d+)", output)
    if not match:
        raise RuntimeError(
            f"Failed to find max edges in output.\n \
            Return Code: {res.exit}\n \
            Stdout: '{output}'\n \
            Stderr: '{res.stderr.decode()}'"
        )

    return int(match.group(1))


async def main(args: Namespace):
    max_edges = await init()
    print("found ", max_edges, " max_edges")
    ipc_queue = engine.IPCTokenQueue(8, max_edges)
    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.fifo(), [engine.StrategyBuilder.table_guard()], ipc_queue, 42
    )

    args.seeds = None

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
