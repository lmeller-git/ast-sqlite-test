ARG USE_ASAN=false

# prep
FROM theosotr/sqlite3-test:latest AS chef

USER root

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    curl \
    python3 \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    curl -LsSf https://astral.sh/uv/install.sh | sh && \
    curl -LsSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

ENV PATH="/root/.cargo/bin:${PATH}"
ENV PATH="/root/.local/bin:${PATH}"
ENV UV_LINK_MODE=copy
ENV CARGO_TARGET_DIR=/app/target
ENV MATURIN_RELEASE=true

RUN cargo install --locked cargo-chef

# plan
FROM chef AS planner
WORKDIR /app

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# building image

FROM chef AS builder

WORKDIR /home/test/sqlite3-src/build

# prebuild sqlite target

RUN ../configure --enable-all && make sqlite3.c

# ASAN + pc_guard leads to random segfaults due to address randomization (i think). since we do (probably) not control the run flags, we cannot set security opts and thus we cannot disable adde randomization. In this case we cannot use ASAN
RUN SQLITE_FLAGS="-DSQLITE_ENABLE_MATH_FUNCTIONS=1 \
        -DSQLITE_ENABLE_FTS4=1 \
        -DSQLITE_ENABLE_FTS5=1 \
        -DSQLITE_ENABLE_GEOPOLY=1 \
        -DSQLITE_ENABLE_RTREE=1 \
        -DSQLITE_ENABLE_SESSION=1 \
        -DSQLITE_ENABLE_PREUPDATE_HOOK=1 \
        -DSQLITE_ENABLE_FTS3=1 \
        -DSQLITE_ENABLE_FTS3_PARENTHESIS=1 \
        -DSQLITE_ENABLE_JSON1=1 \
        -DSQLITE_ENABLE_STAT4=1 \
        -DSQLITE_ENABLE_UPDATE_DELETE_LIMIT=1 \
        -DSQLITE_ENABLE_COLUMN_METADATA=1 \
        -DSQLITE_ENABLE_DBSTAT_VTAB=1 \
        -DSQLITE_ENABLE_EXPLAIN_COMMENTS=1 \
        -DSQLITE_ENABLE_UNKNOWN_SQL_FUNCTION=1 \
        -DSQLITE_ENABLE_STMTVTAB=1 \
        -DSQLITE_ENABLE_DBPAGE_VTAB=1 \
        -DSQLITE_ENABLE_BYTECODE_VTAB=1 \
        -DSQLITE_ENABLE_OFFSET_SQL_FUNC=1" && \
    \
    if [ "$USE_ASAN" = "true" ]; then \
        EXTRA_FLAGS="-fsanitize=address"; \
    else \
        EXTRA_FLAGS=""; \
    fi && \
    \
    clang -O3 $SQLITE_FLAGS $EXTRA_FLAGS \
        -fsanitize-coverage=trace-pc-guard \
        -c sqlite3.c shell.c


# prebuild dependencies

WORKDIR /app

RUN uv venv /app/.venv
ENV VIRTUAL_ENV=/app/.venv
ENV PATH="/app/.venv/bin:$PATH"
ENV PYO3_PYTHON=/app/.venv/bin/python


COPY pyproject.toml uv.lock README.md ./
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-install-project --no-dev

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json

# build lsf-hooks

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY ./crates ./crates
COPY ./extern ./extern
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p lsf-hooks && \
    cp /app/target/release/liblsf_hooks.a /tmp/liblsf_hooks.a

# build sqlite with pc_guard

WORKDIR /home/test/sqlite3-src/build

RUN clang -O3 $EXTRA_FLAGS -fsanitize-coverage=trace-pc-guard -o ./sqlite3 sqlite3.o shell.o -Wl,--whole-archive /tmp/liblsf_hooks.a -Wl,--no-whole-archive

WORKDIR /app

COPY test-db test-db-internal /usr/bin/
RUN chmod +x /usr/bin/test-db /usr/bin/test-db-internal

RUN mkdir -p /app/docker_out/crashes /app/docker_out/queries

# build lsf
COPY python/lib_sf python/lib_sf

RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/app/target \
    uv sync --frozen --no-dev --no-editable

COPY . .

ENV UV_NO_SYNC=1
ENV PYTHONPATH=/app/python

# remove lib_sf to use the wheel installed by uv instead. Python prefers improting from local path
RUN rm -rf /app/python/lib_sf

VOLUME ["/app/docker_out"]
ENTRYPOINT []
