build:
    uvx poetry run maturin develop --release

build-debug:
    uvx poetry run maturin develop

run: build
    python python/tester/main.py

run-debug: build-debug
    python python/tester/main.py

test: build test-rust test-py

test-rust: build
    cargo test -p lib-sf --no-default-features --locked --all-targets
    cargo test --exclude lib-sf --exclude sqlparser --exclude sqlparser_derive --workspace --locked --all-features --all-targets

test-py: build
    uvx poetry run pytest

lint:
    cargo clippy --no-deps
    uvx poetry run ruff check python/
