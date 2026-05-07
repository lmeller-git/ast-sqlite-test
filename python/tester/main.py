from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
import platform
import uvloop

from lib_sf.lib_sf import TestableEntry
from tester.event_loop import CONCURRENCY_LIMIT, fuzzing_loop, N_ORACLES
from tester.exec import init, run_single_mutation
from tester.oracle import oracle_worker
from tester.persistent_worker import SQLiteWorker, PLATFORM
from tester.rules import make_ruleset_havoc, make_ruleset_semantic, make_ruleset_structural

RNG = 42


async def main(args: Namespace):
    if args.disable_addr_randomization:
        PLATFORM = platform.machine()

    max_edges = await init(args.test_path)
    print("found ", max_edges, " max_edges")
    ipc_queue = engine.IPCTokenQueue(CONCURRENCY_LIMIT, max_edges)
    oracle_queue = asyncio.Queue(1024)

    corpus_scheduler_body = engine.MABBody()
    rule_scheduler_body = engine.MABBody()
    havoc_rule_scheduler_body = engine.MABBody()
    struct_rule_scheduler_body = engine.MABBody()
    sem_rule_scheduler_body = engine.MABBody()

    if args.save_to is not None:
        corpus_handler = engine.CorpusManagerBuilder.dynamic_cache(
            engine.DiskCacheBuilder.sql_saver(
                engine.DiskCacheBuilder.blob(args.save_to), args.save_to
            )
        )
    else:
        corpus_handler = engine.CorpusManagerBuilder.in_memory()

    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.batched(
            engine.SchedulerBuilder.weighted_ucb1(corpus_scheduler_body)
        ),
        corpus_handler,
        engine.CorpusMinimizerBuilder.greedy_coverage(max_edges),
        [engine.StrategyBuilder.table_guard()],
        ipc_queue,
        [
            corpus_scheduler_body,
            rule_scheduler_body,
            havoc_rule_scheduler_body,
            sem_rule_scheduler_body,
            struct_rule_scheduler_body,
        ],
        RNG,
    )

    oracle_tasks = [oracle_worker(oracle_queue, args.oracle_path) for _ in range(N_ORACLES)]

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

    # TODO: force add guarded queries back to engine or skip this entirely

    mutation_engine.clear_strategies()

    [
        mutation_engine.add_strategy(strat)
        for strat in [
            engine.StrategyBuilder.ucb1(
                rule_scheduler_body,
                [
                    engine.StrategyBuilder.splice_in(),
                    make_ruleset_havoc(havoc_rule_scheduler_body),
                    make_ruleset_semantic(sem_rule_scheduler_body),
                    make_ruleset_structural(struct_rule_scheduler_body),
                ],
                1,
            ),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_guard(), 0.7),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_name_guard(), 0.7),
        ]
    ]

    # snapshot = mutation_engine.snapshot()

    # tasks = [
    #     run_single_mutation(
    #         TestableEntry.from_raw(entry.clone_raw()),
    #         ipc_queue,
    #         mutation_engine,
    #         oracle_queue,
    #         init_workers,
    #         args.test_path,
    #     )
    #     for entry in snapshot
    # ]

    # r = await asyncio.gather(*tasks)

    # for worker in init_workers.values():
    # await worker.close()

    # print(f"Done executing {r.__len__()} setup queries", flush=True)

    # mutation_engine.chore()

    print("\n===========\ninit done, entering loop\n==================\n")

    _ = await asyncio.gather(
        fuzzing_loop(mutation_engine, ipc_queue, oracle_queue, args.stop_at, args.test_path),
        *oracle_tasks,
    )


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    _ = parser.add_argument("--stop_at", default=10000, type=int)
    _ = parser.add_argument("--save_to", default=None, type=str)
    _ = parser.add_argument("--test_path", default="/home/test/sqlite3-src/build/sqlite3")
    _ = parser.add_argument("--oracle_path", default="/usr/bin/sqlite3-3.39.4")
    _ = parser.add_argument("--disable-addr-randomization", default=False, type=bool)
    args = parser.parse_args()
    uvloop.run(main(args))
