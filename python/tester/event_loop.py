from lib_sf import engine
import asyncio
import sys
from tensorboardX import SummaryWriter
import time

from tester.tb_logger import metrics_logger
from tester.exec import run_single_mutation
from tester.persistent_worker import SQLiteWorker, TestCapture


async def fuzzing_loop(
    mutation_engine: engine.Engine,
    ipc_queue: engine.IPCTokenQueue,
    oracle_queue: asyncio.PriorityQueue[tuple[int, TestCapture | None]],
    stop_at: int,
    track_stats: bool,
    test_path: str,
):
    if track_stats:
        writer = SummaryWriter(log_dir="docker_out/perf_out/runs/fuzzer_stats")
        # Shared state for the background metrics task
        stats = {"mutations": 0, "commits": 0, "exec_s": 0.0, "rust_s": 0.0, "tokens_in_use": 0}

    workers: dict[int, SQLiteWorker] = {}
    active_tasks: set[asyncio.Task[None]] = set()
    CONCURRENCY_LIMIT = 8
    TASK_QUEUE_LIMIT = CONCURRENCY_LIMIT * 3
    epoch = 0

    if track_stats:
        metrics_task = asyncio.create_task(metrics_logger(writer, stats, CONCURRENCY_LIMIT * 2))

    while True:
        if len(active_tasks) < TASK_QUEUE_LIMIT / 2:
            t0 = time.perf_counter()
            batch = mutation_engine.mutate_batch(TASK_QUEUE_LIMIT - len(active_tasks))
            if track_stats:
                stats["rust_s"] += time.perf_counter() - t0

            for entry in batch.into_members():
                task = asyncio.create_task(
                    run_single_mutation(
                        entry,
                        ipc_queue,
                        mutation_engine,
                        oracle_queue,
                        workers,
                        stats if track_stats else None,
                        test_path,
                    )
                )
                active_tasks.add(task)
            epoch += 1

            if epoch % 2000 == 0:
                print(f"epoch {epoch}\nCorpus size: {mutation_engine.corpus_size()}")
                if track_stats:
                    t_gc = time.perf_counter()
                mutation_engine.gc()
                if track_stats:
                    stats["rust_s"] += time.perf_counter() - t_gc

        if not active_tasks:
            continue

        _done, active_tasks = await asyncio.wait(active_tasks, return_when=asyncio.FIRST_COMPLETED)

        if mutation_engine.corpus_size() >= stop_at:
            print(f"Hit {stop_at} queries")
            _ = await asyncio.gather(*active_tasks, return_exceptions=True)
            for worker in workers.values():
                await worker.close()

            _ = await oracle_queue.put((sys.maxsize, None))
            if track_stats:
                metrics_task.cancel()
                writer.close()
            return
