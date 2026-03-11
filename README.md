# ast-sqlite-test
Project 1 for the course "Automated Software Testing" at ETHZ Spring 2026

## Usage
To setup the backend locally run any of

```bash
  just build-debug
```

```bash
  just build
```

```bash
  maturin develop
```

```bash
  maturin develop --release
```

For building a shareable object:

```bash
  uvx poetry build
```

Now the tester can be run using:

```bash
  python python/tester/main.py
```

## References

This project contains code adapted from https://github.com/wseaton/sqloxide.
Adapted files:
  - crates/ffi/src/lib.rs
  - crates/ffi/src/visitor.rs
  - python/lib_sql_fuzzer/lib_sql_fuzzer.pyi
