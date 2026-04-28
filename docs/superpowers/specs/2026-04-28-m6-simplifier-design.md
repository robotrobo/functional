# M6 (partial) — Compile-time Simplifier: η + Inlining

**Date:** 2026-04-28
**Status:** Approved — ready for implementation plan.
**Scope:** First two of four M6 sub-tasks from the language design spec (`2026-04-26-minimal-lambda-language-design.md`, §5 Phase C / M6). Defers strictness analysis and Church-numeral primitive elision.

## 1. Goal

Reduce CBN runtime work by performing β-equivalent program-to-program rewrites at compile time, between `inline_defs` and `to_db`. Speed is the success criterion: step counts on benchmark programs must not increase under the simplifier and should measurably decrease on prelude-heavy code.

Pedagogical learning (per design spec §5 M6): "compiler passes as program-to-program transformations."

## 2. Pipeline placement

```
parse  →  inline_defs  →  simplify  →  to_db  →  cbn::nf  →  to_named  →  print
                          ^^^^^^^^
                          new pass
```

Same insertion point in REPL.

The simplifier operates on the named `Expr` AST. It reuses the existing `subst` and `free_vars` from `src/eval.rs`. It is a closed transformation: input and output are α-equivalent.

## 3. Rewrite rules

All rules are sound under call-by-name with thunk sharing — i.e., they do not duplicate work.

1. **η-reduction.** `\x. f x  →  f` when `x ∉ FV(f)`.
2. **Dead-arg drop.** `(\x. M) N  →  M` when `x ∉ FV(M)`. Eliminates a thunk allocation for an unused binding.
3. **Var inline.** `(\x. M) N  →  M[x := N]` when `N` is a `Var`. Substituting a variable cannot duplicate work.
4. **Linear inline.** `(\x. M) N  →  M[x := N]` when `x` occurs at most once in `M` *and that occurrence is not underneath a λ in `M`*. The "not under λ" guard is essential: without it, a single syntactic occurrence under a λ that gets applied multiple times would force `N` repeatedly post-substitution, where the original CBN thunk would have evaluated it once. Counterexample if dropped: `(\x. \y. x) heavy 1; (\x. \y. x) heavy 2` — heavy is one shared thunk before, two evaluations after.

Explicitly excluded:
- General β (would duplicate work — that is the runtime's job).
- Abs inlining without the linear guard (code-size blow-up *and* potential work duplication).
- Any rewrite that grows the term.

## 4. Algorithm

Bottom-up single-pass rewriter, iterated to fixpoint at the top level.

```rust
pub fn simplify(e: &Expr) -> Expr;
```

Internal structure:
- `step(e: &Expr) -> Expr` recurses into children, then applies whichever of rules 1–4 matches at the current node.
- `simplify` calls `step` in a loop, comparing against the previous iteration via `alpha_eq` (reuse `eval::alpha_eq`); stops on no change. Hard cap at 1000 iterations as a defensive bound — in practice expected to converge in single-digit passes.

New helper in `simplify.rs`:

```rust
enum Occ { Zero, OneSafe, Many }
fn occurs(e: &Expr, x: &str) -> Occ;
```

`OneSafe` ≡ exactly one occurrence of `x` in `e` and not under a `λ` in `e`. `Many` collapses both "≥2 occurrences" and "1 occurrence under a λ"; both block rule 4.

`subst` is reused unmodified from `eval.rs`.

## 5. CLI / toggle

`main.rs` gains a `--no-simplify` flag. When set, the simplifier is bypassed (pipeline reverts to `inline_defs → to_db → cbn::nf`). Default behavior: simplify is on.

REPL: optional `:simplify on|off` command. Nice-to-have, not required for the milestone.

## 6. Measurement

A `benches/` directory using `criterion` (new dev-dependency). Three benches:

- `fact 8`, `fact 10`
- `length` of a 100-element Church-encoded list
- `fib 12` (Church-encoded)

Each benchmark runs the program twice — simplify on vs. off — and reports both wall time and CBN step count. Step count is deterministic and is the assertion target: **simplify-on step count ≤ simplify-off step count** for every benchmark. Wall-time numbers are reported, not asserted.

Requires exposing steps-consumed from `cbn::Budget` (currently tracks remaining; add a getter for `consumed = max - remaining`).

## 7. Testing

Three layers, all built on existing infrastructure.

**Unit tests in `simplify.rs`.** For each rule, one positive test (rule fires on canonical input) and one negative test (rule does *not* fire on its canonical counterexample). Rule 4's "under a λ" guard gets the dedicated `(\x. \y. x) heavy` regression test.

**α-equivalence property over the existing test corpus.** For every prelude integration test currently in `tests/`, run with simplify on and with simplify off; assert results are α-equivalent (reuse `eval::alpha_eq`). This is the soundness check — semantics preserved.

**Benchmark step-count assertion.** As described in §6.

## 8. Out of scope

- M6 part 3: strictness analysis.
- M6 part 4: Church-numeral primitive elision.
- Switching defs from textual substitution to wrapping (`(\f. main) body`) — separate refactor.
- η-expansion or any size-growing rewrite.
- Inlining heuristics beyond the four rules above (e.g., size-bounded Abs inlining, profile-driven decisions).
