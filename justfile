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
    uv run pytest

lint:
    cargo clippy --no-deps
    uv run ruff check python/

run-docker:
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run --security-opt seccomp=unconfined -v $(pwd)/docker_out:/app/docker_out --init -it --rm ast-sqlite-fuzzer //usr/bin/test-db-internal

run-docker-it:
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run --security-opt seccomp=unconfined -v $(pwd)/docker_out:/app/docker_out --init -it --rm ast-sqlite-fuzzer /bin/bash

run-docker-perf-it:
    docker build --build-arg USE_ASAN=true -t ast-sqlite-fuzzer .
    docker run -p 6006:6006 --cap-add=SYS_PTRACE --security-opt seccomp=unconfined --privileged -v $(pwd)/docker_out:/app/docker_out --init -it --rm ast-sqlite-fuzzer /bin/bash

run-flamegraoph:
    setarch -R $(uname -m) uvx py-spy record -o docker_out/perf_out/flamegraph.svg --native -- .venv/bin/python python/tester/main.py --stop_at 5000 --seeds /app/seeds

run-tracer:
    setarch -R $(uname -m) uvx viztracer --log_async -- .venv/bin/python python/tester/main.py --stop_at 200 --seeds /app/seeds
