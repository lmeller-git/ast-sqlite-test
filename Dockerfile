FROM theosotr/sqlite3-test:latest

USER root
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    curl \
    python3

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

WORKDIR /home/test/sqlite3-src
RUN ./configure && make sqlite3.c

WORKDIR /app

COPY test-db.sh /usr/bin/test-db
RUN chmod +x /usr/bin/test-db
RUN mkdir -p /app/sqlite3

COPY pyproject.toml uv.lock ./
RUN uv sync --no-install-project

COPY . /app/

RUN just build-target && just build

ENTRYPOINT []

CMD ["/usr/bin/test-db"]
