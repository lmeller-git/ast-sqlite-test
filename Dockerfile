# prep
FROM theosotr/sqlite3-test:latest AS chef

USER root

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    curl \
    python3 \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN cargo install --locked cargo-chef

RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"


# plan
FROM chef AS planner
WORKDIR /app

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# building image

FROM chef AS builder

WORKDIR /home/test/sqlite3-src/build

RUN make clean && \
    CC="clang" \
    CFLAGS="-O3 -fsanitize=address -fsanitize-coverage=trace-pc-guard" \
    LDFLAGS="-fsanitize=address" \
    ../configure

RUN ASAN_OPTIONS="detect_leaks=0" make sqlite3.c

# prebuild dependencies

WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
COPY ./extern /app/extern
RUN cargo chef cook --release --recipe-path recipe.json

# overwrite cargo chef output for extern/ deps
COPY ./extern /app/extern
RUN cargo build --release

# build sqlite with pc_guard

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY ./crates /app/crates

RUN cargo build --release -p lsf-hooks

WORKDIR /home/test/sqlite3-src/build
# RUN rm -f sqlite3 && \
#     ASAN_OPTIONS="detect_leaks=0" make LDFLAGS="-fsanitize=address -Wl,--whole-archive /app/target/release/liblsf_hooks.a -Wl,--no-whole-archive -lpthread -ldl -lm"
RUN clang -O3 \
    -DSQLITE_ENABLE_RTREE=1 \
    -DSQLITE_ENABLE_FTS3=1 \
    -DSQLITE_ENABLE_FTS3_PARENTHESIS=1 \
    -DSQLITE_ENABLE_FTS4=1 \
    -DSQLITE_ENABLE_FTS5=1 \
    -DSQLITE_ENABLE_GEOPOLY=1 \
    -DSQLITE_ENABLE_JSON1=1 \
    -DSQLITE_ENABLE_MATH_FUNCTIONS=1 \
    -DSQLITE_ENABLE_STAT4=1 \
    -DSQLITE_ENABLE_UPDATE_DELETE_LIMIT=1 \
    -DSQLITE_ENABLE_COLUMN_METADATA=1 \
    -DSQLITE_ENABLE_DBSTAT_VTAB=1 \
    -DSQLITE_ENABLE_SESSION=1 \
    -DSQLITE_ENABLE_PREUPDATE_HOOK=1 \
    -fsanitize=address \
    -fsanitize-coverage=trace-pc-guard \
    -o ./sqlite3 \
    /home/test/sqlite3-src/build/sqlite3.c \
    /home/test/sqlite3-src/build/shell.c \
    -Wl,--whole-archive /app/target/release/liblsf_hooks.a -Wl,--no-whole-archive \
    -lpthread -ldl -lm

# build lsf
WORKDIR /app

RUN cargo build --release

# test-db

WORKDIR /app
COPY test-db.sh /usr/bin/test-db
RUN chmod +x /usr/bin/test-db


RUN mkdir -p /app/docker_out/crashes /app/docker_out/queries

# python deps

COPY pyproject.toml uv.lock README.md LICENSE ./
RUN uv sync --no-install-project

# remainder + build

COPY . .

RUN just build

ENV UV_NO_SYNC=1

VOLUME ["/app/docker_out"]
ENTRYPOINT []
