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
    mean   : 19.333%
    stddev : 0.187%
    min    : 18.874%
    max    : 19.656%
    range  : 0.782%

  QPM       (valid runs: 20/20)
    mean   : 210078.567
    stddev : 25141.478
    min    : 160351.162
    max    : 256927.777
    range  : 96576.615

  rt err rates       (valid runs: 20/20)
    mean   : 0.400
    stddev : 0.041
    min    : 0.319
    max    : 0.475
    range  : 0.156

## long run config

set stop_at to 30k to override short_run flag

uv run perf_eval.py  "setarch -R $(uname -m) just run-docker_ --seeds /app/seeds --save_to docker_out/queries --stop_at 30000 --eval_requirement true" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  Coverage  (valid runs: 20/20)
    mean   : 19.366%
    stddev : 0.216%
    min    : 18.965%
    max    : 19.771%
    range  : 0.806%

  QPM       (valid runs: 20/20)
    mean   : 179322.995
    stddev : 58372.498
    min    : 115363.309
    max    : 370573.442
    range  : 255210.133

  rt err rates       (valid runs: 20/20)
    mean   : 0.552
    stddev : 0.026
    min    : 0.505
    max    : 0.601
    range  : 0.096

# Rust engine

cd perf/dry && uv run ../../dry_perf_eval.py "cargo run --release" --n 20

════════════════════════════════════════════════════════════
  SUMMARY  (20 runs)
════════════════════════════════════════════════════════════
  long  → rules   (valid: 20/20)
    mean   : 315445.042
    stddev : 5774.790
    min    : 301461.870
    max    : 324846.043
    range  : 23384.173

  long  → engine  (valid: 20/20)
    mean   : 150823.073
    stddev : 6508.291
    min    : 140084.545
    max    : 168407.751
    range  : 28323.206

  short → rules   (valid: 20/20)
    mean   : 665286.758
    stddev : 14617.088
    min    : 638266.696
    max    : 703608.898
    range  : 65342.202

  short → engine  (valid: 20/20)
    mean   : 310146.523
    stddev : 43369.353
    min    : 198900.392
    max    : 384152.021
    range  : 185251.629


# throughput criterion

cargo bench -p dry

fuzzer_throughput/mutate_only_short
                        time:   [6.5831 ms 6.8305 ms 7.0808 ms]
                        thrpt:  [9.0386 Kelem/s 9.3697 Kelem/s 9.7218 Kelem/s]

fuzzer_throughput/engine_full_short
                        time:   [26.944 ms 28.622 ms 30.342 ms]
                        thrpt:  [2.1093 Kelem/s 2.2361 Kelem/s 2.3753 Kelem/s]

fuzzer_throughput/mutate_only_long
                        time:   [12.316 ms 12.816 ms 13.336 ms]
                        thrpt:  [4.7991 Kelem/s 4.9937 Kelem/s 5.1964 Kelem/s]

fuzzer_throughput/engine_full_long
                        time:   [22.328 ms 24.235 ms 26.177 ms]
                        thrpt:  [2.4449 Kelem/s 2.6408 Kelem/s 2.8664 Kelem/s]
