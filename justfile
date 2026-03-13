build:
    uvx poetry run maturin develop --release

build-debug:
    uvx poetry run maturin develop

test: build
    uvx poetry run pytest
    cargo test --workspace

lint:
    cargo clippy --no-deps
    uvx poetry run ruff check python/
