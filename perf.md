# Perf

All evals were run from a interactivfe docker container, started with just run-docker-it
All evals were run on a machine with the following specs:
CPU: AMD Ryzen 7 9800X3D 8-Core Processor

# Python timing

## short run config

uv run perf_eval.py "setarch -R x86_64 just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 10000 --eval_requirement true" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  Coverage  (valid runs: 20/20)
    mean   : 19.161%
    stddev : 0.254%
    min    : 18.480%
    max    : 19.528%
    range  : 1.048%

  QPM       (valid runs: 20/20)
    mean   : 105272.544
    stddev : 21937.308
    min    : 65367.924
    max    : 148285.472
    range  : 82917.548

  rt err rates       (valid runs: 20/20)
    mean   : 0.517
    stddev : 0.049
    min    : 0.429
    max    : 0.619
    range  : 0.190

### gcov

Summary coverage rate:
  source files: 4
  lines.......: 39.4% (30787 of 78157 lines)
  functions...: 44.7% (1888 of 4228 functions)
  branches....: 31.7% (16573 of 52334 branches)

## long run config

set stop_at to 30k to override short_run flag

uv run perf_eval.py  "setarch -R $(uname -m) just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 30000 --eval_requirement true" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  Coverage  (valid runs: 20/20)
    mean   : 18.541%
    stddev : 0.215%
    min    : 18.098%
    max    : 18.972%
    range  : 0.874%

  QPM       (valid runs: 20/20)
    mean   : 154012.358
    stddev : 19253.010
    min    : 115728.903
    max    : 185971.752
    range  : 70242.849

  rt err rates       (valid runs: 20/20)
    mean   : 0.677
    stddev : 0.024
    min    : 0.639
    max    : 0.735
    range  : 0.096

Summary coverage rate:
  source files: 5
  lines.......: 39.8% (31286 of 78600 lines)
  functions...: 44.7% (1895 of 4242 functions)
  branches....: 31.9% (16728 of 52422 branches)

# Rust engine

cd perf/dry && uv run ../../dry_perf_eval.py "cargo run --release" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  long  → rules   (valid: 20/20)
    mean   : 408888.605
    stddev : 4902.822
    min    : 400052.482
    max    : 416943.652
    range  : 16891.170

  long  → engine  (valid: 20/20)
    mean   : 195975.082
    stddev : 11080.215
    min    : 174805.679
    max    : 212871.254
    range  : 38065.575

  short → rules   (valid: 20/20)
    mean   : 624207.114
    stddev : 10209.231
    min    : 609010.877
    max    : 643252.526
    range  : 34241.649

  short → engine  (valid: 20/20)
    mean   : 375575.190
    stddev : 38052.051
    min    : 319876.016
    max    : 481140.600
    range  : 161264.584

  aggressive → rules   (valid: 20/20)
    mean   : 585906.560
    stddev : 56637.443
    min    : 520019.673
    max    : 726927.864
    range  : 206908.191

  aggressive → engine  (valid: 20/20)
    mean   : 318550.411
    stddev : 23713.906
    min    : 282302.263
    max    : 375180.739
    range  : 92878.476

  generic → rules   (valid: 20/20)
    mean   : 1514529.312
    stddev : 29075.793
    min    : 1476678.259
    max    : 1577196.850
    range  : 100518.591

  generic → engine  (valid: 20/20)
    mean   : 942413.201
    stddev : 53182.304
    min    : 833789.875
    max    : 1062098.619
    range  : 228308.744

# throughput criterion

cargo bench -p dry

fuzzer_throughput/mutate_only_short
                        time:   [7.2156 ms 7.3408 ms 7.4673 ms]
                        thrpt:  [8.5707 Kelem/s 8.7184 Kelem/s 8.8697 Kelem/s]

fuzzer_throughput/engine_full_short
                        time:   [19.023 ms 20.002 ms 21.024 ms]
                        thrpt:  [3.0441 Kelem/s 3.1996 Kelem/s 3.3644 Kelem/s]

fuzzer_throughput/mutate_only_long
                        time:   [9.6505 ms 9.9434 ms 10.240 ms]
                        thrpt:  [6.2502 Kelem/s 6.4364 Kelem/s 6.6318 Kelem/s]


fuzzer_throughput/engine_full_long
                        time:   [20.001 ms 21.622 ms 23.349 ms]
                        thrpt:  [2.7410 Kelem/s 2.9599 Kelem/s 3.1999 Kelem/s]

fuzzer_throughput/mutate_only_long_aggressive
                        time:   [7.1575 ms 8.2412 ms 9.4170 ms]
                        thrpt:  [6.7962 Kelem/s 7.7659 Kelem/s 8.9416 Kelem/s]

fuzzer_throughput/engine_full_long_aggressive
                        time:   [11.257 ms 13.958 ms 16.843 ms]
                        thrpt:  [3.7998 Kelem/s 4.5852 Kelem/s 5.6855 Kelem/s]

fuzzer_throughput/mutate_only_long_generic
                        time:   [2.5983 ms 2.9004 ms 3.2284 ms]
                        thrpt:  [19.824 Kelem/s 22.066 Kelem/s 24.631 Kelem/s]

fuzzer_throughput/engine_full_long_generic
                        time:   [4.6578 ms 5.2330 ms 5.8350 ms]
                        thrpt:  [10.968 Kelem/s 12.230 Kelem/s 13.740 Kelem/s]
