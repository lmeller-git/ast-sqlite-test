from lib_sf import engine
from argparse import ArgumentParser, Namespace


def main(args: Namespace):
    ipc_queue = engine.IPCTokenQueue(1, 1)

    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.fifo(), [engine.StrategyBuilder.table_guard()], ipc_queue, 42
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

    next_gen = mutation_engine.mutate_batch(8)

    for raw in next_gen.into_members():
        token = ipc_queue.pop()
        if token is not None:
            mutation_engine.commit_test_result(raw, engine.TestResult(0, token))

    mutation_engine.clear_strategies()
    [
        mutation_engine.add_strategy(strat)
        for strat in [engine.StrategyBuilder.splice_in(), engine.StrategyBuilder.table_scrambler()]
    ]

    snapshot = mutation_engine.snapshot()

    for entry in snapshot:
        print(entry.to_sql_string())


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    _ = parser.add_argument("--seeds", default=None, type=str)
    args = parser.parse_args()
    main(args)
