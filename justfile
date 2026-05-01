build:
    uv sync
    uv run maturin develop --release --uv

build-debug:
    uv run maturin develop --uv

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
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run --security-opt seccomp=unconfined -v $(pwd)/docker_out:/app/docker_out -u $(id -u):$(id -g) --init -it --rm ast-sqlite-fuzzer /usr/bin/test-db-internal

run-docker-it:
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run --security-opt seccomp=unconfined -v $(pwd)/docker_out:/app/docker_out -u $(id -u):$(id -g) --init -it --rm ast-sqlite-fuzzer /bin/bash

run-docker-perf-it:
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run -p 6006:6006 --cap-add=SYS_PTRACE --security-opt seccomp=unconfined --privileged -v $(pwd)/docker_out:/app/docker_out -u $(id -u):$(id -g) --init -it --rm ast-sqlite-fuzzer /bin/bash


run-flamegraoph:
    uv run py-spy record -o docker_out/perf_out/flamegraph.svg --native -- python python/tester/main.py --stop_at 5000 --disable-addr-randomization true

run-tracer:
    uv run viztracer --log_async python/tester/main.py --stop_at 200 --disable-addr-randomization true

run-with-tensorboard:
    @echo "starting tensorboard in bg. You may connect to it via port 6006"
    uvx --with "setuptools<70" tensorboard --logdir /app/docker_out/perf_out/runs/fuzzer_stats --host 0.0.0.0 --port 6006 >/dev/null 2>&1 &
    # uv run tensorboard --logdir /app/docker_out/perf_out/runs/fuzzer_stats --host 0.0.0.0 --port 6006 > /dev/null 2>&1 &
    uv run python python/tester/main.py --stats true --stop_at 20000 --disable-addr-randomization true
