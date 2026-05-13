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
    mean   : 19.277%
    stddev : 0.200%
    min    : 18.660%
    max    : 19.554%
    range  : 0.894%

  QPM       (valid runs: 20/20)
    mean   : 66642.960
    stddev : 13634.761
    min    : 50472.347
    max    : 102425.031
    range  : 51952.684

  rt err rates       (valid runs: 20/20)
    mean   : 0.489
    stddev : 0.033
    min    : 0.419
    max    : 0.536
    range  : 0.117

## long run config

set stop_at to 30k to override short_run flag

uv run perf_eval.py  "setarch -R $(uname -m) just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 30000 --eval_requirement true" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  Coverage  (valid runs: 20/20)
    mean   : 18.446%
    stddev : 0.208%
    min    : 18.067%
    max    : 18.862%
    range  : 0.795%

  QPM       (valid runs: 20/20)
    mean   : 111707.403
    stddev : 24939.489
    min    : 68546.349
    max    : 163579.306
    range  : 95032.957

  rt err rates       (valid runs: 20/20)
    mean   : 0.576
    stddev : 0.029
    min    : 0.501
    max    : 0.619
    range  : 0.118

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
