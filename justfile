build:
    uv sync
    # uv run maturin develop --release

build-debug:
    uv run maturin develop

run *args: build
    uv run python python/tester/main.py {{args}}

run-debug *args: build-debug
    uv run python python/tester/main.py {{args}}

run-docker_ *args:
    uv run python python/tester/main.py {{args}}

tarball:
    uv build

test: build-debug test-rust test-py

test-rust: build-debug
    cargo test -p lib-sf --no-default-features --locked --all-targets
    cargo test --exclude lib-sf --exclude sqlparser --exclude sqlparser_derive --workspace --locked --all-features --all-targets

test-py: build-debug
    uv  run pytest

lint:
    cargo clippy --no-deps
    uv run ruff check python/


run-docker:
    docker build -t ast-sqlite-fuzzer .
    docker run -v $(pwd)/crashes:/app/crashes -v $(pwd)/queries:/app/queries --init -it --rm ast-sqlite-fuzzer /usr/bin/test-db

run-docker-perf-it:
    docker build -t ast-sqlite-fuzzer .
    docker run -p 6006:6006 --cap-add=SYS_PTRACE --security-opt seccomp=unconfined --privileged -v $(pwd)/crashes:/app/crashes -v $(pwd)/queries:/app/queries -v $(pwd)/perf_out:/app/perf_out --init -it --rm ast-sqlite-fuzzer /bin/bash


run-flamegraoph:
    uv run py-spy record -o perf_out/flamegraph.svg --native -- python python/tester/main.py --stop_at 2000

run-tracer:
    uv run viztracer --log_async python/tester/main.py --stop_at 500

run-with-tensorboard:
    @echo "starting tensorboard in bg. You may connect to it via port 6006"
    uv run tensorboard --logdir /app/perf_out/runs/fuzzer_stats --host 0.0.0.0 --port 6006 > /dev/null 2>&1 &
    uv run python python/tester/main.py --stats true
