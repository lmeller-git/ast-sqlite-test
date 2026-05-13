#!/usr/bin/env python3

# LLM gnereated

"""
Run a command N times, collect final coverage and QPM from stdout,
then report mean, stddev, and range for each metric.
"""

import subprocess
import re
import shutil
import sys
import os
import argparse
import numpy as np

OUTPUT_DIR = "docker_out/queries"

COV_RE = re.compile(r"Total coverage so far:\s*([\d.]+)%")
QPM_RE = re.compile(r"qpm for complete pipeline:\s*([\d.]+)")
RT_ERR_RE = re.compile(r"runtime error rate is \s*([\d.]+)")


def stats(values: list[float]) -> dict[str, float]:
    a = np.array(values)
    return {
        "mean": float(np.mean(a)),
        "stddev": float(np.std(a)),
        "min": float(np.min(a)),
        "max": float(np.max(a)),
        "range": float(np.ptp(a)),
    }


def fmt(label: str, s: dict[str, float], unit: str = "") -> str:
    return (
        f"  {label}\n"
        f"    mean   : {s['mean']:.3f}{unit}\n"
        f"    stddev : {s['stddev']:.3f}{unit}\n"
        f"    min    : {s['min']:.3f}{unit}\n"
        f"    max    : {s['max']:.3f}{unit}\n"
        f"    range  : {s['range']:.3f}{unit}\n"
    )


def clear_output_dir(path: str) -> None:
    if os.path.isdir(path):
        shutil.rmtree(path)
    os.makedirs(path, exist_ok=True)


def run_once(cmd: str, run_idx: int, total: int) -> tuple[float | None, float | None, float | None]:
    print(f"\n{'─' * 60}")
    print(f"  Run {run_idx}/{total}")
    print(f"{'─' * 60}")

    last_cov: float | None = None
    qpm: float | None = None
    rt_rate: float | None = None

    proc = subprocess.Popen(
        cmd,
        shell=True,
        executable="/bin/bash",  # needed for $(uname -m) expansion
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,  # merge stderr so nothing is lost
        text=True,
        bufsize=1,
    )

    for line in proc.stdout:
        sys.stdout.write(line)
        sys.stdout.flush()

        m = COV_RE.search(line)
        if m:
            last_cov = float(m.group(1))

        m = QPM_RE.search(line)
        if m:
            qpm = float(m.group(1))

        m = RT_ERR_RE.search(line)
        if m:
            rt_rate = float(m.group(1))

    _ = proc.wait()

    if proc.returncode != 0:
        print(f"[WARNING] process exited with code {proc.returncode}")

    return last_cov, qpm, rt_rate


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run a command N times and summarise coverage/QPM stats."
    )
    _ = parser.add_argument("cmd", help="Command to execute (quote the whole string)")
    _ = parser.add_argument("--n", type=int, help="Number of runs", default=10)
    _ = parser.add_argument(
        "--outdir",
        default=OUTPUT_DIR,
        help=f"Directory to empty between runs (default: {OUTPUT_DIR})",
    )
    args = parser.parse_args()

    print(f"Command : {args.cmd}")
    print(f"Runs    : {args.n}")
    print(f"Out dir : {args.outdir}")

    covs: list[float] = []
    qpms: list[float] = []
    rt_rates: list[float] = []

    for i in range(1, args.n + 1):
        clear_output_dir(args.outdir)
        cov, qpm, rt_rate = run_once(args.cmd, i, args.n)

        if cov is None:
            print(f"[WARNING] no coverage value found in run {i} — skipping")
        else:
            covs.append(cov)
            print(f"  → final cov : {cov:.3f}%")

        if qpm is None:
            print(f"[WARNING] no QPM value found in run {i} — skipping")
        else:
            qpms.append(qpm)
            print(f"  → qpm       : {qpm:.3f}")

        if rt_rate is None:
            print(f"[WARNING] no rt error rate value found in run {i} — skipping")
        else:
            rt_rates.append(rt_rate)
            print(f"  → rt err rate       : {rt_rate:.3f}")

    print(f"\n{'═' * 60}")
    print(f"  SUMMARY  ({args.n} runs)")
    print(f"{'═' * 60}")

    if covs:
        print(fmt(f"Coverage  (valid runs: {len(covs)}/{args.n})", stats(covs), unit="%"))
    else:
        print("  Coverage : no data collected")

    if qpms:
        print(fmt(f"QPM       (valid runs: {len(qpms)}/{args.n})", stats(qpms)))
    else:
        print("  QPM : no data collected")

    if rt_rates:
        print(fmt(f"rt err rates       (valid runs: {len(rt_rates)}/{args.n})", stats(rt_rates)))
    else:
        print("  rt err rate : no data collected")


if __name__ == "__main__":
    main()
