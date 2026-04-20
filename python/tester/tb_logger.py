import time
import asyncio
from typing import Any
from tensorboardX import SummaryWriter


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
