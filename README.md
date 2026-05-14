![CI Test](https://github.com/lmeller-git/ast-sqlite-fuzzer/actions/workflows/test.yml/badge.svg?branch=main)

# ast-sqlite-fuzzer
Project 1 for the course "Automated Software Testing" at ETHZ Spring 2026

## Usage
The course requirement run (up to 10000 queries) may be run using

```bash
  docker build -t ast-sqlite-fuzzer . &&
  docker run --security-opt seccomp=unconfined -v $(pwd)/docker_out:/app/docker_out --init --rm ast-sqlite-fuzzer /usr/bin/test-db
```

To run on your machine:

```bash
  just run
```

To run in a docker container:

```bash
  just run-docker
```

This will build a docker container and execute `test-db-internal` inside it


## References

This project contains code adapted from https://github.com/wseaton/sqloxide.

Adapted files:
  - crates/ffi/src/lib.rs
  - crates/ffi/src/visitor.rs
  - python/lib_sql_fuzzer/lib_sql_fuzzer.pyi
