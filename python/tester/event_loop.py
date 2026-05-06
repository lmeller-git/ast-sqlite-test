from lib_sf import engine
import asyncio
import sys

from lib_sf.lib_sf import TestableEntry
from tester.exec import run_single_mutation
from tester.persistent_worker import SQLiteWorker, TestCapture

CONCURRENCY_LIMIT = 16


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: asyncio.PriorityQueue[tuple[int, TestCapture | None]],
    stop_at: int,
    test_path: str,
):
    workers: dict[int, SQLiteWorker] = {}
    active_tasks: set[asyncio.Task[None]] = set()
    testable_queries: list[TestableEntry] = []
    epoch = 0

    while True:
        if len(testable_queries) < CONCURRENCY_LIMIT * 2:
            batch = mutation_engine.mutate_batch(CONCURRENCY_LIMIT * 4 - len(testable_queries))
            for entry in batch.into_members():
                testable_queries.append(entry)
            if epoch % 2000 == 0:
                print(f"epoch {epoch}\nCorpus size: {mutation_engine.corpus_size()}")
                mutation_engine.chore()
            epoch += 1

        to_spawn = CONCURRENCY_LIMIT - len(active_tasks)

        if len(testable_queries) < to_spawn:
            continue

        for _ in range(to_spawn):
            task = asyncio.create_task(
                run_single_mutation(
                    testable_queries.pop(),
                    ipc_queue,
                    mutation_engine,
                    oracle_queue,
                    workers,
                    test_path,
                )
            )
            active_tasks.add(task)


        _done, active_tasks = await asyncio.wait(active_tasks, return_when=asyncio.FIRST_COMPLETED)

        if mutation_engine.corpus_size() >= stop_at:
            print(f"Hit {stop_at} queries")
            _ = await asyncio.gather(*active_tasks, return_exceptions=True)
            for worker in workers.values():
                await worker.close()
            _ = await oracle_queue.put((sys.maxsize, None))
            return
