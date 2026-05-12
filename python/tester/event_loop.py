from lib_sf import engine
import asyncio
import time

from lib_sf.lib_sf import TestableEntry
from tester.keyword_coverage import KeywordCoverageRecorder
from tester.exec import CONCURRENCY_LIMIT, run_single_mutation
from tester.persistent_worker import SQLiteWorker, TestCapture

QUERY_STASH = CONCURRENCY_LIMIT * 8
N_ORACLES = 4


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: asyncio.Queue[TestCapture | None],
    stop_at: int,
    stop_time: int | None,
    test_path: str,
    eval_requirement: bool = False,
    keyword_coverage: KeywordCoverageRecorder | None = None,
):
    workers: dict[int, SQLiteWorker] = {}
    active_tasks: set[asyncio.Task[None]] = set()
    testable_queries: list[TestableEntry] = []
    epoch = 0
    total = 0
    is_done = False
    short_run = stop_at <= 20000

    while True:
        if len(testable_queries) < QUERY_STASH / 2:
            batch = mutation_engine.mutate_batch(QUERY_STASH - len(testable_queries))
            generated_queries = batch.into_members()
            if keyword_coverage is not None:
                for query in generated_queries:
                    keyword_coverage.record(query.to_sql_string())
            testable_queries += generated_queries

        to_spawn = CONCURRENCY_LIMIT - len(active_tasks)

        if len(testable_queries) < to_spawn:
            continue

        for _ in range(to_spawn):
            entry = testable_queries.pop()

            # save 10k queries, comment out in an actual run. this is only to save the first 10k for grading
            if eval_requirement and total < 10000:
                with open(f"docker_out/queries/query_{total}.sql", "w") as f:
                    _ = f.write(entry.to_sql_string())
                    total += 1
            elif eval_requirement and not is_done:
                # break at 10k
                is_done = True
                print("Hit 10k queries")
                break

            if not is_done:
                task = asyncio.create_task(
                    run_single_mutation(
                        entry, ipc_queue, mutation_engine, oracle_queue, workers, test_path
                    )
                )
                active_tasks.add(task)

        if not short_run and epoch % 2000 == 0:
            print(f"epoch {epoch}\nCorpus size: {mutation_engine.corpus_size()}")
            mutation_engine.chore()

        _done, active_tasks = await asyncio.wait(active_tasks, return_when=asyncio.FIRST_COMPLETED)

        epoch += 1

        if (
            is_done
            or mutation_engine.corpus_size() >= stop_at
            or (stop_time is not None and time.time() >= stop_time)
        ):
            if eval_requirement:
                print(f"Hit {total} generated queries")
            else:
                print(f"Hit {mutation_engine.corpus_size()} queries")
            _ = await asyncio.gather(*active_tasks, return_exceptions=True)
            for worker in workers.values():
                await worker.close()
            _ = await oracle_queue.join()
            for _ in range(N_ORACLES):
                oracle_queue.put_nowait(None)
            return
