import time
import asyncio
import csv
from typing import Any
from tensorboardX import SummaryWriter

from lib_sf import engine


async def metrics_logger(writer: SummaryWriter, stats: dict[Any, Any], concurrency_limit: int):
    last_time = time.perf_counter()
    last_muts = 0
    last_commits = 0
    last_exec_s = 0.0
    last_rust_s = 0.0
    step = 0

    while True:
        await asyncio.sleep(1.0)
        now = time.perf_counter()
        dt = now - last_time

        d_muts = stats["mutations"] - last_muts
        d_commits = stats["commits"] - last_commits
        d_exec_s = stats["exec_s"] - last_exec_s
        d_rust_s = stats["rust_s"] - last_rust_s

        # 1. Throughput
        writer.add_scalar("Throughput/Mutations_per_sec", d_muts / dt, step)
        writer.add_scalar("Throughput/Commits_per_sec", d_commits / dt, step)

        # 2. Worker Uptime
        worker_uptime_pct = (d_exec_s / (dt * concurrency_limit)) * 100
        writer.add_scalar("Efficiency/Worker_Uptime_pct", worker_uptime_pct, step)

        # 3. Main Thread Starvation
        rust_block_pct = (d_rust_s / dt) * 100
        writer.add_scalar("Efficiency/Main_Thread_Rust_Block_pct", rust_block_pct, step)

        # 4. Token Utilization (Gauge)
        token_util_pct = (stats["tokens_in_use"] / concurrency_limit) * 100
        writer.add_scalar("Efficiency/Token_Utilization_pct", token_util_pct, step)

        last_time = now
        last_muts = stats["mutations"]
        last_commits = stats["commits"]
        last_exec_s = stats["exec_s"]
        last_rust_s = stats["rust_s"]
        step += 1


async def csv_logger(
    scheduler_hook: engine.SchedulerHook | None,
    strategy_scheduler_hook: engine.SchedulerHook | None,
):
    if scheduler_hook is None or strategy_scheduler_hook is None:
        return

    scheduler_writer = csv.writer(open("docker_out/perf_out/scheduler_stats.csv", "w", buffering=1))
    strategy_writer = csv.writer(open("docker_out/perf_out/strategy_stats.csv", "w", buffering=1))

    scheduler_writer.writerow(
        [
            "tick",
            "name",
            "attempts",
            "accepted",
            "cov_increase",
            "syntax_err",
            "rating",
            "probability",
        ]
    )
    strategy_writer.writerow(
        [
            "tick",
            "name",
            "attempts",
            "accepted",
            "cov_increase",
            "syntax_err",
            "rating",
            "probability",
        ]
    )

    while True:
        if not scheduler_hook.dirty() and not strategy_scheduler_hook.dirty():
            await asyncio.sleep(0.1)
            continue

        strategy_stats = strategy_scheduler_hook.drain()
        scheduler_stats = scheduler_hook.drain()

        for s in strategy_stats:
            strategy_writer.writerow(
                [
                    s.epoch,
                    s.name,
                    s.self_attempts[0],
                    s.accepted[0],
                    s.cov_increases[0],
                    s.syntax_err[0],
                    s.rating[0],
                    s.rating_as_prob[0],
                ]
            )
        for s in scheduler_stats:
            for id, attempts, accepted, cov_increase, syntax_err, rating in zip(
                s.meta, s.self_attempts, s.accepted, s.cov_increases, s.syntax_err, s.rating
            ):
                scheduler_writer.writerow(
                    [
                        s.epoch,
                        id,
                        attempts,
                        accepted,
                        cov_increase,
                        syntax_err,
                        rating,
                        rating,
                    ]
                )
