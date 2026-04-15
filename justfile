build: #build-target
    uv sync
    uv run maturin develop --release

build-debug: #build-target
    uv run maturin develop

build-hooks:
    cargo build --release -p lsf-hooks

build-target: build-hooks
    clang -O3 \
        -fsanitize-coverage=trace-pc-guard \
        -o out/sqlite3_fuzz \
        /home/test/sqlite3-src/sqlite3.c \
        /home/test/sqlite3-src/shell.c \
        target/release/liblsf_hooks.a \
        -lpthread -ldl -lm

run *args: build
    uv run python python/tester/main.py {{args}}

run-debug *args: build-debug
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
