# lc — minimal lambda-calculus language

Pure untyped λ-calculus, written in Rust. Standard library written in the language itself.

## Usage

```bash
cargo run                                    # REPL with prelude.lc preloaded
cargo run -- examples/factorial/fact_3.lc    # run a file
cargo test                                   # run all tests
```

The REPL has line editing and history. The pretty-printer detects Church
numerals, booleans, and lists, and renders them as `3`, `true`, `[1, 2, 3]`
instead of raw λ-terms.

## Layout

- `src/` — Rust interpreter (parser, evaluator, pretty-printer, REPL)
- `lib/prelude.lc` — standard library in pure λ
- `examples/` — sample programs
- `tests/` — integration tests

## Status

Phase A complete: working call-by-name normal-order tree-walking interpreter.
Phase B (call-by-need) and Phase C (optimizations, abstract machines) planned.

See [docs/superpowers/specs](docs/superpowers/specs) for the full design.
