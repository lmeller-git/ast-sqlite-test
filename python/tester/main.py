from lib_sf import engine, restore_ast
from oracle import SqliteExecutor, DuplicateDetector
from argparse import ArgumentParser


def main(args):
    mutation_engine = engine.Engine(
        engine.SchedulerBuilder.fifo(), [engine.StrategyBuilder.table_guard()], 42
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
            # Example of a simple, "correct" query
            engine.SeedGeneratorBuilder.literal("SELECT 1;"),
            engine.SeedGeneratorBuilder.literal("SELECT 1;"),
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

    # Initialize executor and duplicate detector
    executor = SqliteExecutor(timeout=5.0)
    dedup = DuplicateDetector()
    
    next_gen = mutation_engine.mutate_batch(8)
    selected = []
    for raw in next_gen.into_members():
        # Execute query and check for validity
        query = restore_ast(raw.as_ast())
        result = executor.execute(query)
        
        # Keep only successful, non-duplicate queries
        if result.is_success() and not dedup.is_duplicate(result):
            selected.append(raw.into_corpus_entry())
            print(f"[BATCH 0] SUCCESS: hash={result.result_hash}, rows={result.output_rows}")
        elif result.is_error():
            dedup.record_error(result)
    
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
        selected = []
        for raw in next_gen.into_members():
            query = restore_ast(raw.as_ast())
            result = executor.execute(query)
            
            if result.is_success() and not dedup.is_duplicate(result):
                selected.append(raw.into_corpus_entry())
            elif result.is_error():
                dedup.record_error(result)
        
        mutation_engine.commit_generation(engine.SelectedGeneration(selected))

    snapshot = mutation_engine.snapshot()

    print(f"\nsnapshot with length {snapshot.__len__()}:\n")
    
    # Test snapshot queries against oracle to show results
    print("SNAPSHOT QUERY EXECUTION RESULTS:")
    print("-" * 60)
    successful_count = 0
    for i, member in enumerate(snapshot):
        query = restore_ast(member.as_ast())
        result = executor.execute(query)
        status_symbol = "✓" if result.is_success() else "✗"
        print(f"{status_symbol} Query {i+1}: {result.status.value} ({result.execution_time:.4f}s)")
        if result.is_success():
            print(f"  Output: {result.output_rows} rows, hash={result.result_hash}")
            successful_count += 1
            # Also add to dedup tracker for accurate stats
            dedup.is_duplicate(result)
        else:
            print(f"  Error: {result.error_type} - {result.error_message[:60]}")
        print(f"  SQL: {query[:80]}{'...' if len(query) > 80 else ''}")
        print()
    
    # Print oracle statistics
    stats = dedup.get_stats()
    print("\n" + "="*60)
    print("ORACLE STATISTICS")
    print("="*60)
    print(f"Unique successful queries: {stats['unique_results']}")
    print(f"Successful queries in snapshot: {successful_count}/{len(list(snapshot))}")
    print(f"Error breakdown: {stats['error_types']}")
    print("="*60)


def add(n1: int, n2: int) -> int:
    return n1 + n2


if __name__ == "__main__":
    parser = ArgumentParser()
    parser.add_argument("--seeds", default=None, type=str)
    args = parser.parse_args()
    main(args)
