import os
import time
from lib_sf.lib_sf import TestableEntry
from lib_sf import engine
from argparse import ArgumentParser, Namespace
import asyncio
import uvloop

from tester.event_loop import fuzzing_loop, N_ORACLES
from tester.exec import CONCURRENCY_LIMIT, init, run_single_mutation, return_rt_err_rate
from tester.oracle import oracle_worker
from tester.persistent_worker import SQLiteWorker
from tester.rules import make_aggressive_ruleset, make_full_ruleset, make_shortrunning_ruleset

RNG = 42


async def main(args: Namespace):
    if args.timeout_hours is not None:
        start_time = time.time()
        end_time = args.timeout_hours * 3600.0 + start_time

    short_run = args.stop_at <= 20000

    max_edges = await init(args.test_path)
    print("found ", max_edges, " max_edges")
    ipc_queue = engine.IPCTokenQueue(CONCURRENCY_LIMIT, max_edges)
    oracle_queue = asyncio.Queue(4096)

    scheduler_config = engine.MABConfig.new_default()

    if short_run:
        scheduler_config.exploration_constant = 0.25
        scheduler_config.max_accepted_syntax_err = 0.1

    corpus_scheduler_body = engine.MABBody()
    rule_scheduler_body = engine.MABBody()
    havoc_rule_scheduler_body = engine.MABBody()
    struct_rule_scheduler_body = engine.MABBody()
    sem_rule_scheduler_body = engine.MABBody()
    generator_body = engine.MABBody()
    reducer_body = engine.MABBody()
    increaser_body = engine.MABBody()
    outer_body = engine.MABBody()

    corpus_scheduler_body.with_config(scheduler_config)
    outer_body.with_config(scheduler_config)
    rule_scheduler_body.with_config(scheduler_config)
    havoc_rule_scheduler_body.with_config(scheduler_config)
    struct_rule_scheduler_body.with_config(scheduler_config)
    sem_rule_scheduler_body.with_config(scheduler_config)
    generator_body.with_config(scheduler_config)
    reducer_body.with_config(scheduler_config)
    increaser_body.with_config(scheduler_config)

    # use InMemory Corpus for short run, as we will gneraet small amount of queries
    if args.save_to is not None and not short_run:
        disk_cache = (
            engine.DiskCacheBuilder.sql_saver(
                engine.DiskCacheBuilder.blob(args.save_to), args.save_to
            )
            if not args.eval_requirement  # sql queriesa re saved separately for requirement fulfilling runs
            else engine.DiskCacheBuilder.blob(args.save_to)
        )

        corpus_handler = engine.CorpusManagerBuilder.dynamic_cache(disk_cache)
    else:
        corpus_handler = engine.CorpusManagerBuilder.in_memory()

    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.batched(
            engine.SchedulerBuilder.fast_weigthed_ucb1(corpus_scheduler_body)
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
            generator_body,
            reducer_body,
            increaser_body,
            outer_body,
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
            make_aggressive_ruleset(
                outer_body,
                rule_scheduler_body,
                havoc_rule_scheduler_body,
                sem_rule_scheduler_body,
                generator_body,
                reducer_body,
                increaser_body,
            )
            if not short_run
            else make_shortrunning_ruleset(
                rule_scheduler_body,
                havoc_rule_scheduler_body,
                struct_rule_scheduler_body,
                sem_rule_scheduler_body,
                increaser_body,
            ),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.table_guard(), 0.6),
            engine.StrategyBuilder.randomize(engine.StrategyBuilder.force_ident(), 0.1),
        ]
    ]

    snapshot = mutation_engine.snapshot()

    mutation_engine.clear()

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

    mutation_engine.chore()

    print("\n===========\ninit done, entering loop\n==================\n", flush=True)

    if args.eval_requirement and args.save_to is not None:
        os.makedirs(args.save_to, exist_ok=True)

    now = time.time()

    _ = await asyncio.gather(
        fuzzing_loop(
            mutation_engine,
            ipc_queue,
            oracle_queue,
            args.stop_at,
            None if args.timeout_hours is None else end_time,
            args.test_path,
            args.eval_requirement,
            args.save_to,
        ),
        *oracle_tasks,
    )

    duration = time.time() - now
    qpm = (10000.0 / duration) * 60.0
    print(f"qpm for complete pipeline: {qpm:.3f}", flush=True)
    print(f"runtime error rate is {return_rt_err_rate(10000):.3f}", flush=True)


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    _ = parser.add_argument("--stop_at", default=10000, type=int)
    _ = parser.add_argument("--save_to", default=None, type=str)
    _ = parser.add_argument("--test_path", default="/home/test/sqlite3-src/build/sqlite3")
    _ = parser.add_argument("--oracle_path", default="/usr/bin/sqlite3")
    _ = parser.add_argument("--timeout_hours", default=None, type=float)
    _ = parser.add_argument("--eval_requirement", default=False, type=bool)
    args = parser.parse_args()
    uvloop.run(main(args))
