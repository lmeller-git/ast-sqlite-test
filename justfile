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
