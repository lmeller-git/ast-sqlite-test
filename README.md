# ast-sqlite-test
Project 1 for the course "Automated Software Testing" at ETHZ Spring 2026

## Usage
To run on your machine:

```bash
just run
```

To run in a docker container:

```bash
just run-docker
```

This will build a docker container and execute `test-db-internal.sh` inside it


## References

This project contains code adapted from https://github.com/wseaton/sqloxide.

Adapted files:
  - crates/ffi/src/lib.rs
  - crates/ffi/src/visitor.rs
  - python/lib_sql_fuzzer/lib_sql_fuzzer.pyi
