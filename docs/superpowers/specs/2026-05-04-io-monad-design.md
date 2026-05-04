# IO Monad — Design

**Date:** 2026-05-04
**Status:** Spec, ready for plan
**Predecessors:** 2026-04-30-hm-type-inference-advisory, 2026-04-30-primitive-nat

## Goal

Add side effects to the typed lambda calculus via a monomorphic, built-in `IO` monad — Haskell-style. After this work, the target program

```
main = bind readNat (\n. print (mul n 2))
```

reads a natural number from stdin and prints `2 * n` to stdout.

## Non-goals

- **User-definable monads.** No type classes, no higher-kinded types. `IO` is the only monad in the language. A user wanting `Maybe` would write a one-off `maybe_bind` once we have ADTs; there is no plan for a polymorphic `bind` that dispatches across types.
- **Mutable state, refs, exceptions.** Out of scope. `IO` here only covers stdin/stdout.
- **Do-notation, infix operators.** No `do { ... }`, no `>>=`, no `>>`. Sequencing is written as nested `bind` (verbose but honest). Sugar can be added later without breaking this design.
- **Concurrency / async.** Out of scope.

## User-facing surface

### New types

- `Unit` — primitive type. Single value `()`.
- `IO a` — primitive type constructor parameterised over `a`. Opaque: there is no way to extract a value from `IO a` except via `bind`.

### New keywords / primitives

| keyword   | type                                          |
|-----------|-----------------------------------------------|
| `pure`    | `forall a. a -> IO a`                         |
| `bind`    | `forall a b. IO a -> (a -> IO b) -> IO b`     |
| `print`   | `Nat -> IO Unit`                              |
| `readNat` | `IO Nat`                                      |

### New literal

- `()` parses to `Expr::UnitLit`, type `Unit`.

### `main` typing rule

In **file mode** (`cargo run -- foo.lc`), `main` *must* have type `IO a` for some `a`. A `main : Nat` (or any non-IO type) is a type error: `main must have type 'IO a', got 'Nat'`.

In **REPL mode**, no such restriction. Each entered expression is evaluated independently; if its type is `IO a` the runtime executes it, otherwise it pretty-prints as today.

The `--no-typecheck` CLI flag bypasses the main-typing check (and all other type checks), as it does today.

### Prelude additions

```
def seq  = \a. \b. bind a (\_. b)            -- IO a -> IO b -> IO b
def fmap = \f. \m. bind m (\x. pure (f x))   -- (a -> b) -> IO a -> IO b
```

Both are ordinary user-level definitions on top of `bind` / `pure`. `seq` is "do A, ignore result, do B"; `fmap` lifts a pure function into IO.

## Type system

### `src/types.rs` additions

```rust
pub enum Type {
    Var(TVar),
    Arrow(Box<Type>, Box<Type>),
    Nat,
    Unit,                    // NEW
    IO(Box<Type>),           // NEW
}
```

`IO` is a special-cased one-argument constructor in the same way `Arrow` is a special-cased two-argument constructor. We do **not** add general type constructors or kinds.

### Unification

Three new cases:
- `unify(Unit, Unit)` → ok, no substitution.
- `unify(IO a, IO b)` → recurse on `unify(a, b)`.
- All other `Unit` / `IO` cross-constructor pairs → type error with the standard "expected X, got Y" message.

### `ftv`

- `Unit` has no free type variables.
- `IO t` has the same free type variables as `t`.

### Pretty-printing types

- `Unit` → `Unit`.
- `IO Nat` → `IO Nat`.
- `IO (Nat -> Nat)` → `IO (Nat -> Nat)` (parenthesise inner arrows / IOs).
- `Nat -> IO Unit` → `Nat -> IO Unit` (no extra parens needed; arrow is right-associative and `IO` binds tighter).

### `instantiate_prim`

New entries:

| primitive | scheme                                          |
|-----------|-------------------------------------------------|
| `Pure`    | `forall a. a -> IO a`                           |
| `Bind`    | `forall a b. IO a -> (a -> IO b) -> IO b`       |
| `Print`   | `Nat -> IO Unit`                                |
| `ReadNat` | `IO Nat`                                        |

### `UnitLit` inference

`infer_expr(UnitLit) = (empty subst, Unit)`. Mirrors `NatLit`.

### Top-level `main` check (file mode only)

After `infer_program` succeeds, if the program has a `main`:
1. Take the inferred type `t`.
2. Unify `t` with `IO ?` for a fresh tvar `?`.
3. If unification fails, emit `main must have type 'IO a', got '<t>'` and halt before evaluation.

This check lives in `src/main.rs`, not in `infer_program` itself, so library callers (REPL, tests) are unaffected.

### Generalisation / value restriction

Because the language has no mutable refs and `IO` is opaque, the standard ML value restriction does not apply. Let-generalisation stays as-is. **If `IORef` is ever added, this must be revisited.**

## Evaluator

### `Value` additions in `src/cbn.rs`

```rust
pub enum Value {
    Nat(u64),
    Closure(...),
    Neu(...),
    StuckApp(...),
    Unit,                          // NEW
    IOAction(Rc<IOAction>),        // NEW — opaque to pretty-printer
}
```

### `IOAction` — operation tree

```rust
pub enum IOAction {
    Pure(ThunkRef),                       // pure x → yield x
    Print(ThunkRef),                      // print n → force n, write line, yield ()
    ReadNat,                              // read stdin line → yield Nat
    Bind(Rc<IOAction>, ThunkRef),         // bind m k → run m, apply k, run that
}
```

### WHNF rules for new primitives

Implemented in the existing primitive-saturation code path:

- `Pure` (arity 1, saturated): produce `Value::IOAction(Pure(thunk_x))`. Argument is **not** forced — matches Haskell's lazy `return`.
- `Bind` (arity 2, saturated): force first arg to WHNF, expect `Value::IOAction`; produce `Value::IOAction(Bind(action, thunk_k))`. Continuation `k` stays a thunk.
- `Print` (arity 1, saturated): produce `Value::IOAction(Print(thunk_n))`. `n` forced **only when the action runs**.
- `ReadNat` (arity 0): evaluating `Expr::Prim(ReadNat)` directly yields `Value::IOAction(ReadNat)`. New code path — first arity-0 primitive in the language.
- `Expr::UnitLit` → `Value::Unit`.

**Building the IO tree is pure.** No side effect happens during evaluation to WHNF — only when the runtime walks the tree.

### Runtime driver — `run_io`

```rust
fn run_io(action: &IOAction) -> Value {
    let mut current = action.clone();
    loop {
        match &*current {
            Pure(t)  => return force(t),
            Print(t) => { let n = force_nat(t); println!("{}", n); return Value::Unit }
            ReadNat  => return Value::Nat(read_stdin_line_as_u64()),
            Bind(m, k) => {
                let a = run_io(m);                     // run inner action
                let next_thunk = apply(k, a);          // continuation gets the value
                current = force_to_ioaction(next_thunk);
            }
        }
    }
}
```

Implemented as a loop, not actual recursion, so deeply-bound chains (e.g. `seq (print 1) (seq (print 2) ...)` × 10 000) do not blow the stack.

### Top-level dispatch

In `src/main.rs` (file mode) and `src/repl.rs` (REPL):

1. Inline defs → simplify → normalize to **WHNF only** (not full NF — full NF would try to recurse into the action tree, which is wrong).
2. If WHNF is `Value::IOAction(_)`, call `run_io`.
3. Final `Value` from `run_io`: if `Value::Unit`, print nothing; otherwise pretty-print.

For non-IO `main` in file mode, the type-check from earlier already rejects.
For non-IO REPL expressions, fall through to today's pretty-print path.

### Pretty-printer

`Value::IOAction(_)` and unforced thunks of `IO` type print as `<IO action>` if they ever leak (e.g. REPL `:env` listing a def whose body is `IO _`). They never appear in normal program output, because the runtime consumes them before pretty-printing.

### Stdin parse failure

`readNat` reads one line. If the line cannot be parsed as `u64`:

```
runtime error: readNat: could not parse '<line>' as Nat
```

to stderr, exit 1. No `Maybe` involved.

## Tests

### Type tests (`tests/io_types_test.rs`)

- `pure 1 : IO Nat`
- `pure () : IO Unit`
- `readNat : IO Nat`
- `print 5 : IO Unit`
- `bind readNat print : IO Unit`
- `bind (pure 1) (\n. pure (succ n)) : IO Nat`
- `bind 1 print` → "expected `IO a`, got `Nat`"
- `print ()` → "expected `Nat`, got `Unit`"
- File-mode `main = 1` → "main must have type `IO a`, got `Nat`"

### Evaluation tests (`tests/io_eval_test.rs`)

Drive `run_io` directly with a mock-stdin / capture-stdout harness:

- `pure 42` → result `Value::Nat(42)`, no output.
- `print 7` → result `Value::Unit`, stdout `"7\n"`.
- `bind (pure 1) (\n. pure (succ n))` → result `Value::Nat(2)`, no output.
- `bind readNat print` with stdin `"5\n"` → result `Value::Unit`, stdout `"5\n"`.
- `seq (print 1) (print 2)` → stdout `"1\n2\n"` in that order.
- Target program `bind readNat (\n. print (mul n 2))` with stdin `"21\n"` → stdout `"42\n"`.

### Laziness tests

- `pure (fix (\x. x))` — building the action does not diverge.
- `bind m k` does not force `k` until `m` finishes (test by passing a `k` whose body would diverge if forced eagerly).

### Stack-safety test

- A 10 000-deep `seq` chain (built programmatically in the test harness) does not stack-overflow when run.

### Stdin parse failure test

- `readNat` with stdin `"hello"` → exits 1, stderr contains `"could not parse"`.

## Rollout

Six commits, in order:

1. Add `Type::Unit`, `Type::IO`, `Expr::UnitLit`, `PrimOp::{Pure,Bind,Print,ReadNat}`. Parser + types + inference. Type-only tests pass; eval tests not yet present.
2. Add `Value::Unit`, `Value::IOAction`, evaluator WHNF rules for the four primitives. Eval tests for action-building (no driver yet) pass.
3. Add `run_io` driver and main-mode dispatch in `src/main.rs` and `src/repl.rs`. Stdin/stdout harness tests pass.
4. Add prelude `seq` and `fmap`. Tests for them pass.
5. Audit `.lc` files (`examples/`, integration tests) for `main : non-IO`. Update / delete as appropriate.
6. Final pass: full `cargo test` green; manual REPL smoke; manual file-mode smoke with the target program.

## Open questions / future work

- **`do`-notation.** Parser-level sugar for nested `bind`. Trivial once we want it; design unaffected.
- **Infix operators (`>>=`, `>>`).** Same — parser-level. Not blocking.
- **Strings.** `print` only accepts `Nat`. A `printStr : String -> IO Unit` requires a primitive `String` type, which is its own design.
- **`Bool`.** Independent feature; not required for this work, but `Unit` here paves the way for primitive `Bool` and `IfBool` ergonomically.
- **`IORef`.** Mutable cells. Adding them re-opens the value-restriction question (see Type system → Generalisation).
- **User-defined monads.** Requires type classes or HKTs. Tracked separately.
