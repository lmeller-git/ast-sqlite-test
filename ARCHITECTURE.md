# General

Responsibilities split across python and rust layers.

Rust interface may be used via crate `ffi` as `lib_sql_fuzzer`

## Rust

Implements the mutation engine and AST.
The API is defined in crate `ffi`.

### Engine

Keeps track of and manages generated tests

Applies and validates mutations


## Python

User facing interface,

Strategy decisions for engine.

Instrument extern tool calls (gcov, sql testing, ...).



