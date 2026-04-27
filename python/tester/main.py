from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
import os

from lib_sf.lib_sf import TestableEntry
from tester.event_loop import fuzzing_loop
from tester.exec import init, run_single_mutation
from tester.oracle import oracle
from tester.persistent_worker import SQLiteWorker


async def main(args: Namespace):
    max_edges = await init(args.test_path)
    print("found ", max_edges, " max_edges")
    ipc_queue = engine.IPCTokenQueue(8, max_edges)
    oracle_queue = asyncio.PriorityQueue(1024)

    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.adaptive_weighted_random(),
        [engine.StrategyBuilder.table_guard()],
        ipc_queue,
        42,
    )

    # populate coverage map with "basic edges"

    mutation_engine.populate(
        [
            engine.SeedGeneratorBuilder.literal(
                "CREATE TABLE A(x); INSERT INTO A VALUES(1); SELECT x FROM A;"
            )
        ]
    )

    snapshot = mutation_engine.snapshot()

    # currently the inital queries are run outside of event loop, as we do not want to run them through the mutation logic.
    # The reason for this is that the scheduler would heavily prioritize the first run seeds, leading to the others only getting run much later

    init_workers: dict[int, SQLiteWorker] = {}

    for entry in snapshot:
        _ = await run_single_mutation(
            TestableEntry.from_raw(entry.clone_raw()),
            ipc_queue,
            mutation_engine,
            oracle_queue,
            init_workers,
            args.test_path,
        )

    mutation_engine.clear()

    # Run seeds

    print(f"Populating engine with seeds from {args.seeds}\n", flush=True)

    mutation_engine.populate(
        [
            engine.SeedGeneratorBuilder.dir_reader(args.seeds)
            if args.seeds is not None
            else engine.SeedGeneratorBuilder.literal("CREATE TABLE B; SELECT a FROM B")
        ]
    )

    oracle_task = asyncio.create_task(oracle(oracle_queue, args.oracle_path))

    # TODO: force add guarded queries back to engine or skip this entirely

    mutation_engine.clear_strategies()

    [
        mutation_engine.add_strategy(strat)
        for strat in [
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.splice_in()),
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.random_sampler(
                    1,
                    1,
                    [
                        engine.StrategyBuilder.scheduled(
                            engine.StrategyBuilder.tree_mutate_stmt(
                                engine.TreeMutatorOperation.null_random()
                            )
                        ),
                        engine.StrategyBuilder.scheduled(
                            engine.StrategyBuilder.tree_mutate_stmt(
                                engine.TreeMutatorOperation.shuffle_two()
                            )
                        ),
                    ],
                )
            ),
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.random_sampler(
                    1,
                    1,
                    [
                        engine.StrategyBuilder.scheduled(
                            engine.StrategyBuilder.tree_mutate_stmt(
                                engine.TreeMutatorOperation.null_random()
                            )
                        ),
                        engine.StrategyBuilder.scheduled(
                            engine.StrategyBuilder.tree_mutate_stmt(
                                engine.TreeMutatorOperation.shuffle_two()
                            )
                        ),
                    ],
                )
            ),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.recursive_expand_expr()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.num_bounds()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.op_flip()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.null_inject()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.type_cast()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.set_ops()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.sub_query()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.relation_shuffle()),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_guard(), 0.6),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_name_guard(), 0.6),
        ]
    ]

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())

    tasks = [
        run_single_mutation(
            TestableEntry.from_raw(entry.clone_raw()),
            ipc_queue,
            mutation_engine,
            oracle_queue,
            init_workers,
            args.test_path,
        )
        for entry in snapshot
    ]

    r = await asyncio.gather(*tasks)

    for worker in init_workers.values():
        await worker.close()

    print(f"Done executing {r.__len__()} setup queries", flush=True)

    mutation_engine.gc()

    print("\n===========\ninit done, entering loop\n==================\n")

    _ = await asyncio.gather(
        fuzzing_loop(mutation_engine, ipc_queue, oracle_queue, args.stop_at, args.test_path),
        oracle_task,
    )

    if args.save_to is not None:
        print(f"Saving {mutation_engine.corpus_size()} queries to {args.save_to}\n", flush=True)

        snapshot = mutation_engine.snapshot()

        os.makedirs(args.save_to, exist_ok=True)
        for i, query in enumerate(snapshot):
            with open(f"{args.save_to}/query_{i}.sql", "w", encoding="utf-8") as f:
                _ = f.write(query.to_sql_string())


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    _ = parser.add_argument("--stop_at", default=10000, type=int)
    _ = parser.add_argument("--save_to", default=None, type=str)
    _ = parser.add_argument("--test_path", default="/home/test/sqlite3-src/build/sqlite3")
    _ = parser.add_argument("--oracle_path", default="/usr/bin/sqlite3-3.39.4")
    args = parser.parse_args()
    asyncio.run(main(args))
