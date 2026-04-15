FROM theosotr/sqlite3-test:latest

USER root
RUN apt-get update && apt-get install -y \
    clang \
    curl \
    build-essential \
    python3

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

WORKDIR /home/test/sqlite3-src
RUN ./configure && make sqlite3.c

WORKDIR /app
COPY . /app/


RUN just build
ENTRYPOINT []
CMD ["just", "run", "/home/test/seeds"]
