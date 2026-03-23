from lib_sf import engine, restore_ast


def main():
    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.fifo(), [engine.StrategyBuilder.table_guard()], 42
    )

    mutation_engine.populate(
        [
            engine.SeedGeneratorBuilder.literal(
                "\
                CREATE TABLE b;\
                SELECT a FROM b;\
                "
            ),
            engine.SeedGeneratorBuilder.literal(
                "\
                CREATE TABLE t0(c0 REAL UNIQUE);\
                INSERT INTO t0(c0) VALUES (3175546974276630385);\
                SELECT 3175546974276630385 < c0 FROM t0;\
                SELECT 1 FROM t0 WHERE 3175546974276630385 < c0;\
                "
            ),
            engine.SeedGeneratorBuilder.literal(
                "\
                CREATE TABLE map_integer (id INT, name);\
                INSERT INTO map_integer VALUES(1,'a');\
                CREATE TABLE map_text (id TEXT, name);\
                INSERT INTO map_text VALUES('4','e');\
                CREATE TABLE data (id TEXT, name);\
                INSERT INTO data VALUES(1,'abc');\
                INSERT INTO data VALUES('4','xyz');\
                CREATE VIEW idmap as SELECT * FROM map_integer UNION SELECT * FROM map_text;\
                CREATE TABLE mzed AS SELECT * FROM idmap;\
                PRAGMA automatic_index=ON;\
                SELECT * FROM data JOIN idmap USING(id);\
                "
            ),
        ]
    )

    next_gen = mutation_engine.mutate_batch(8)
    selected = [raw.into_corpus_entry() for raw in next_gen.into_members()]
    mutation_engine.commit_generation(engine.SelectedGeneration(selected))

    mutation_engine.clear_strategies()
    [
        mutation_engine.add_strategy(strat)
        for strat in [
            engine.StrategyBuilder.splice_in(),
            engine.StrategyBuilder.table_scrambler(),
        ]
    ]

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
