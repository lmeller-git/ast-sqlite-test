# Perf

All evals were run from a interactivfe docker container, started with just run-docker-it
All evals were run on a machine with the following specs:
CPU: AMD Ryzen 7 9800X3D 8-Core Processor

# Python timing

## short run config

uv run perf_eval.py "setarch -R x86_64 just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 10000 --eval_requirement true" --n 20

  Coverage  (valid runs: 20/20)
    mean   : 19.228%
    stddev : 0.168%
    min    : 18.965%
    max    : 19.725%
    range  : 0.760%

  QPM       (valid runs: 20/20)
    mean   : 216910.600
    stddev : 82890.876
    min    : 75743.393
    max    : 355204.962
    range  : 279461.569

## long run config

set stop_at to 30k to override short_run flag

uv run perf_eval.py  "setarch -R $(uname -m) just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 30000 --eval_requirement true" --n 20

  Coverage  (valid runs: 20/20)
    mean   : 19.505%
    stddev : 0.120%
    min    : 19.314%
    max    : 19.734%
    range  : 0.420%

  QPM       (valid runs: 20/20)
    mean   : 185296.108
    stddev : 44643.080
    min    : 116228.463
    max    : 318835.269
    range  : 202606.806

# Rust engine

cd perf/dry && uv run ../../dry_perf_eval.py "cargo run --release" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  long  → rules   (valid: 20/20)
    mean   : 345433.929
    stddev : 7712.190
    min    : 331540.663
    max    : 361118.485
    range  : 29577.822

  long  → engine  (valid: 20/20)
    mean   : 171871.741
    stddev : 5414.176
    min    : 162048.680
    max    : 179443.615
    range  : 17394.935

  short → rules   (valid: 20/20)
    mean   : 645569.593
    stddev : 19931.385
    min    : 591001.060
    max    : 674267.360
    range  : 83266.300

  short → engine  (valid: 20/20)
    mean   : 318615.446
    stddev : 33660.614
    min    : 259770.233
    max    : 392727.057
    range  : 132956.824



# throughput criterion

fuzzer_throughput/mutate_only_short
                        time:   [6.2968 ms 6.6355 ms 6.9771 ms]
                        thrpt:  [9.1729 Kelem/s 9.6451 Kelem/s 10.164 Kelem/s]

fuzzer_throughput/engine_full_short
                        time:   [20.551 ms 22.831 ms 25.177 ms]
                        thrpt:  [2.5420 Kelem/s 2.8032 Kelem/s 3.1143 Kelem/s]

fuzzer_throughput/mutate_only_long
                        time:   [12.024 ms 12.539 ms 13.073 ms]
                        thrpt:  [4.8956 Kelem/s 5.1041 Kelem/s 5.3225 Kelem/s]

fuzzer_throughput/engine_full_long
                        time:   [18.507 ms 21.986 ms 25.634 ms]
                        thrpt:  [2.4967 Kelem/s 2.9109 Kelem/s 3.4582 Kelem/s]
