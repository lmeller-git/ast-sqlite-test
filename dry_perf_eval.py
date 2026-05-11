#!/usr/bin/env python3
"""
Run a command N times, collect QPM for all four reported metrics:
  long  → rules
  long  → engine
  short → rules
  short → engine
Then report mean, stddev, min, max, and range for each.
"""

# LLM generatedd

import subprocess
import re
import sys
import argparse
import numpy as np

QPM_RE = re.compile(r"queries per minute from 10k for (\w+):\s*([\d.]+)")
CONFIG_RE = re.compile(r"^(long|short) config:")

METRICS = [("long", "rules"), ("long", "engine"), ("short", "rules"), ("short", "engine")]


def stats(values: list[float]) -> dict[str, float]:
    a = np.array(values)
    return {
        "mean": float(np.mean(a)),
        "stddev": float(np.std(a)),
        "min": float(np.min(a)),
        "max": float(np.max(a)),
        "range": float(np.ptp(a)),
    }


def fmt(label: str, s: dict[str, float]) -> str:
    return (
        f"  {label}\n"
        f"    mean   : {s['mean']:.3f}\n"
        f"    stddev : {s['stddev']:.3f}\n"
        f"    min    : {s['min']:.3f}\n"
        f"    max    : {s['max']:.3f}\n"
        f"    range  : {s['range']:.3f}\n"
    )


def run_once(cmd: str, run_idx: int, total: int) -> dict[tuple[str, str], float | None]:
    print(f"\n{'─' * 60}")
    print(f"  Run {run_idx}/{total}")
    print(f"{'─' * 60}")

    # Collect all (config, component) → qpm seen in this run.
    # If a key appears multiple times we keep the last value.
    results: dict[tuple[str, str], float] = {}
    current_config: str | None = None

    proc = subprocess.Popen(
        cmd,
        shell=True,
        executable="/bin/bash",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    for line in proc.stdout:
        # sys.stdout.write(line)
        # sys.stdout.flush()

        m = CONFIG_RE.match(line.strip())
        if m:
            current_config = m.group(1)

        m = QPM_RE.search(line)
        if m and current_config:
            component = m.group(1)  # "rules" or "engine"
            qpm = float(m.group(2))
            results[(current_config, component)] = qpm

    _ = proc.wait()

    if proc.returncode != 0:
        print(f"[WARNING] process exited with code {proc.returncode}")

    # Return one value per expected metric (None if missing)
    return {key: results.get(key) for key in METRICS}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run a command N times and summarise QPM stats for all four configs."
    )
    _ = parser.add_argument("cmd", help="Command to execute (quote the whole string)")
    _ = parser.add_argument("--n", type=int, help="Number of runs", default = 10)
    args = parser.parse_args()

    print(f"Command : {args.cmd}")
    print(f"Runs    : {args.n}")

    # collected[metric_key] = list of floats across runs
    collected: dict[tuple[str, str], list[float]] = {k: [] for k in METRICS}

    for i in range(1, args.n + 1):
        run_results = run_once(args.cmd, i, args.n)

        print()
        for key in METRICS:
            val = run_results[key]
            label = f"{key[0]:5s} → {key[1]}"
            if val is None:
                print(f"  [WARNING] {label} : not found in run {i}")
            else:
                collected[key].append(val)
                print(f"  → {label} : {val:.3f}")

    print(f"\n{'═' * 60}")
    print(f"  SUMMARY  ({args.n} runs)")
    print(f"{'═' * 60}")

    for key in METRICS:
        values = collected[key]
        label = f"{key[0]:5s} → {key[1]:6s}  (valid: {len(values)}/{args.n})"
        if values:
            print(fmt(label, stats(values)))
        else:
            print(f"  {label} : no data collected\n")


if __name__ == "__main__":
    main()
