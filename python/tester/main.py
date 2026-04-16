from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
import os

from tester.event_loop import fuzzing_loop
from tester.exec import init, run_single_mutation
from tester.oracle import oracle


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

    oracle_queue = asyncio.PriorityQueue(1024)

    oracle_task = asyncio.create_task(oracle(oracle_queue))

    # TODO: force add guarded queries back to engine or skip this entirely

    mutation_engine.clear_strategies()
    [
        mutation_engine.add_strategy(strat)
        for strat in [
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.splice_in(), 0.5),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_scrambler(), 0.3),
            engine.StrategyBuilder.random_sampler(
                1,
                5,
                [
                    engine.StrategyBuilder.op_flip(),
                    engine.StrategyBuilder.num_bounds(),
                    engine.StrategyBuilder.null_inject(),
                    engine.StrategyBuilder.type_cast(),
                    engine.StrategyBuilder.set_ops(),
                    engine.StrategyBuilder.sub_query(),
                ],
            ),
        ]
    ]

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())

    tasks = [
        run_single_mutation(entry.clone_raw(), ipc_queue, mutation_engine, oracle_queue)
        for entry in snapshot
    ]

    r = await asyncio.gather(*tasks)

    print(f"Done executing {r.__len__()} setup queries", flush=True)

    mutation_engine.gc()

    print("\n===========\ninit done, entering loop\n==================\n")

    _ = await asyncio.gather(fuzzing_loop(mutation_engine, ipc_queue, oracle_queue), oracle_task)

    print("Saving 10000 queries to ./queries/\n", flush=True)

    snapshot = mutation_engine.snapshot()

    os.makedirs("queries", exist_ok=True)
    for i, query in enumerate(snapshot):
        with open(f"queries/query_{i}.sql", "w", encoding="utf-8") as f:
            _ = f.write(query.to_sql_string())


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    args = parser.parse_args()
    asyncio.run(main(args))
