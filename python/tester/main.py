from lib_sf import engine, restore_ast


def main():
    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.fifo(),
        [engine.StrategyBuilder.random_uppercase(0.6), engine.StrategyBuilder.merger()],
    )
    mutation_engine.populate(
        [
            engine.SeedGeneratorBuilder.literal("SELECT a FROM b"),
            engine.SeedGeneratorBuilder.literal(
                "CREATE TABLE t0(c0 REAL UNIQUE);\
                INSERT INTO t0(c0) VALUES (3175546974276630385);\
                "
            ),
        ]
    )

    for i in range(0, 5):
        next_gen = mutation_engine.mutate_batch(4)
        selected = [raw.into_corpus_entry() for raw in next_gen.into_members()]
        mutation_engine.commit_generation(engine.SelectedGeneration(selected))

    snapshot = mutation_engine.snapshot()

    print("snapshot:\n")
    for member in snapshot:
        print(restore_ast(member.as_ast()), end="\n")


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    main()
