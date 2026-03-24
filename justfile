build:
    uvx poetry run maturin develop --release

build-debug:
    uvx poetry run maturin develop

run *args: build
    python python/tester/main.py {{args}}

run-debug *args: build-debug
    python python/tester/main.py {{args}}

test: build-debug test-rust test-py

test-rust: build-debug
    cargo test -p lib-sf --no-default-features --locked --all-targets
    cargo test --exclude lib-sf --exclude sqlparser --exclude sqlparser_derive --workspace --locked --all-features --all-targets

test-py: build-debug
    uvx poetry run pytest

lint:
    cargo clippy --no-deps
    uvx poetry run ruff check python/
