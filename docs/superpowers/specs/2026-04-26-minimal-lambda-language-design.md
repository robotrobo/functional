# Minimal Lambda-Calculus Language — Design

**Date:** 2026-04-26
**Status:** Design (awaiting user review before implementation planning)

## 1. Goals & Non-Goals

### Goals
- A pedagogical functional programming language whose core is **the pure untyped lambda calculus**, nothing more.
- Build the language and a working interpreter in **Rust**, as a vehicle for learning functional programming, compiler construction, runtime optimization, and the underlying mathematics.
- Provide a **standard library written in the language itself** — booleans, numerals, pairs, recursion, and lists — so that every "feature" beyond the three core forms is an exercise in encoding rather than a primitive.
- Stage the implementation as a sequence of milestones, each of which teaches one specific compiler/runtime concept and produces a working artifact.

### Non-Goals
- A type system. The language is and stays untyped throughout this design.
- Native code emission. The roadmap ends at an abstract machine VM. Native codegen is explicitly out of scope (see §6).
- I/O, side effects, or any form of impurity. The language manipulates lambda terms and prints their normal forms; nothing else.
- Production use. This is a learning artifact.

## 2. The Language

### 2.1 Abstract syntax (the only thing the evaluator sees)

```
e ::= x          -- variable
    | \x. e      -- abstraction
    | e e        -- application
```

Three forms. Anything else in the source — `def`, comments, whitespace — is handled by the parser and never reaches the AST.

### 2.2 Concrete syntax (what the user writes)

```
program     ::= def* expr?
def         ::= "def" identifier "=" expr
expr        ::= atom+                       -- juxtaposition = application, left-assoc
atom        ::= identifier
              | "\" identifier "." expr     -- λ-body extends as far right as possible
              | "(" expr ")"
identifier  ::= [a-zA-Z_][a-zA-Z0-9_]*
comment     ::= "--" until end of line
```

Conventions:
- **Application is left-associative**: `f x y z` parses as `((f x) y) z`.
- **λ-bodies extend as far right as possible**: `\x. f x y` parses as `\x. (f x y)`, not `(\x. f x) y`.
- File extension: `.lc`.
- A file consists of zero or more `def`s followed by an optional final expression to evaluate. If the final expression is omitted, the file is a library file (e.g. `prelude.lc`).
- **No multi-argument λ-sugar.** `\x. \y. M` is written explicitly. (Trivial to add later if desired; deliberately omitted from v1 to keep every λ a real, single-argument λ.)

### 2.3 Semantics

**Reduction strategy: call-by-name normal-order reduction.** To evaluate an expression, repeatedly find the **leftmost-outermost** β-redex and reduce it, until no redex remains (the term is in normal form) or the step limit is exceeded.

**β-reduction:**
```
(\x. M) N → M[x := N]    -- with capture-avoiding substitution
```

Substitution must α-rename bound variables when necessary to avoid capturing free variables in `N`.

**Stopping condition.** The interpreter stops at *weak head normal form* during execution: it does not reduce inside the body of an unapplied lambda when running a program. Full normal form is computed only when explicitly requested (e.g., for printing).

**Errors.** The only runtime error in pure λ-calculus is a free variable referenced at the top level (no `def` for it). The interpreter reports such errors with a source location. Non-termination is a valid program behavior, not an error; the step-limit produces an "exceeded reduction budget" diagnostic but is not semantically distinguished from a longer computation.

### 2.4 Top-level `def`s

`def`s are pure surface sugar. The interpreter substitutes each `def` body into uses of its name in `main` (and in subsequent `def`s) before reducing. Equivalent to wrapping `main` in nested applications. (When call-by-need lands in M5, this becomes proper environment lookup with sharing rather than naive substitution.)

## 3. Implementation Architecture (Rust)

Single crate. Flat module layout:

```
src/
  main.rs        -- CLI entry: file mode + REPL mode
  lexer.rs       -- tokenization (or absorbed into parser.rs if using chumsky)
  parser.rs      -- chumsky-based parser → Program
  ast.rs         -- Expr, Program types
  eval.rs        -- normal-order reduction (M2); replaced by graph reducer in M5
  pretty.rs      -- Expr → String, with Church-numeral / boolean detection
  error.rs       -- error types with span info
lib/
  prelude.lc     -- the stdlib, written in the language itself
tests/
  *.rs           -- integration tests: source string → expected normal form
```

### 3.1 AST representation

```rust
pub enum Expr {
    Var(String),
    Abs(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
}

pub struct Program {
    pub defs: Vec<(String, Expr)>,
    pub main: Option<Expr>,
}
```

`String` for variables in v1; switched to **De Bruijn indices** (`usize`) in M4.

### 3.2 Evaluator structure (v1, M2)

```rust
fn reduce_step(e: &Expr) -> Option<Expr>;   // one leftmost-outermost step, or None
fn normalize(e: Expr, step_limit: usize) -> Result<Expr, EvalError>;
```

`def`s are inlined into `main` before reduction.

### 3.3 Library choices

- **Parser:** [`chumsky`](https://crates.io/crates/chumsky) parser-combinator library. Grammar fits in ~40 lines; error messages are excellent out of the box.
- **REPL line editing:** [`rustyline`](https://crates.io/crates/rustyline) for history, arrow keys, etc.
- **Pretty-printing, AST, evaluator, De Bruijn conversion, abstract machine:** **hand-rolled.** Rule of thumb: anything that's part of the math gets written by hand; everything else gets a library.

## 4. Standard Library (`lib/prelude.lc`)

All written in the language itself — no primitives anywhere. The stdlib is also the primary test corpus.

The "tier" headings below are conceptual groupings for human readers; the actual file is ordered by dependency (since §2.4 substitutes `def`s into *subsequent* defs only, no forward references). Specifically: numerals' `pred`/`sub` and the `shift` helper sit *after* pairs in `prelude.lc`, even though they are conceptually part of the numerals tier. The order on disk is:

```
combinators → booleans → numerals (basic: zero..isZero)
            → pairs    → numerals (pred, sub)  → Y, fact  → lists
```

### Tier 1 — Combinators
```
def id      = \x. x
def const   = \x. \y. x
def flip    = \f. \x. \y. f y x
def compose = \f. \g. \x. f (g x)
```

### Tier 2 — Church booleans
```
def true  = \t. \f. t
def false = \t. \f. f
def if    = \b. \t. \e. b t e
def not   = \p. p false true
def and   = \p. \q. p q p
def or    = \p. \q. p p q
```

### Tier 3a — Church numerals (basic)
```
def zero   = \f. \x. x
def succ   = \n. \f. \x. f (n f x)
def one    = succ zero
def two    = succ one
def add    = \m. \n. \f. \x. m f (n f x)
def mul    = \m. \n. \f. m (n f)
def pow    = \m. \n. n m
def isZero = \n. n (\_. false) true
```

### Tier 4 — Pairs
```
def pair  = \a. \b. \s. s a b
def fst   = \p. p (\a. \b. a)
def snd   = \p. p (\a. \b. b)
def shift = \p. pair (snd p) (succ (snd p))
```

### Tier 3b — Numerals using pairs (`pred`, `sub`)
```
def pred = \n. fst (n shift (pair zero zero))
def sub  = \m. \n. n pred m
```

### Tier 5 — Recursion
```
def Y    = \f. (\x. f (x x)) (\x. f (x x))
def fact = Y (\rec. \n. (isZero n) one (mul n (rec (pred n))))
```

`Y` works under call-by-name; would diverge under call-by-value. This is the moment the evaluation-strategy choice from §2.3 pays off concretely. Note that `fact` uses the boolean directly as a selector (`(isZero n) one (...)`) rather than going through `if`; both are β-equivalent, but the direct form makes it visible that no `if` primitive is needed.

### Tier 6 — Lists (Church-encoded as right folds)
```
def nil    = \c. \n. n
def cons   = \h. \t. \c. \n. c h (t c n)
def isNil  = \l. l (\_. \_. false) true
def foldr  = \c. \n. \l. l c n
def map    = \f. \l. \c. \n. l (\h. c (f h)) n
def filter = \p. \l. \c. \n. l (\h. \r. (p h) (c h r) r) n
def append = \xs. \ys. \c. \n. xs c (ys c n)
def length = \l. l (\_. \r. succ r) zero
```

### Deferred (not in v1 stdlib)
- Sum types (`either`, `maybe`).
- The S combinator and SKI-style basis.
- The Z and Θ fixed-point combinators.
- `head`/`tail` (partial; require `maybe` to express safely).

## 5. Roadmap

The implementation is staged across three phases. Each milestone is a self-contained deliverable that produces a working artifact.

### Phase A — Minimum viable language

**M0: Hello AST.**
Cargo project scaffold; `Expr`, `Program` types; hard-coded AST in `main.rs` reduced by hand-written substitution.
*Learning:* Rust ergonomics for sum types; `Box<Expr>` vs `Rc<Expr>`; the pain that motivates everything that follows.

**M1: Parser via `chumsky`.**
Grammar as combinators producing `Program`. Hand-rolled pretty-printer for round-tripping.
*Learning:* parser-combinator style; error-recovery at parse time; how associativity rules manifest as code.

**M2: Tree-walking evaluator.**
`reduce_step` finds leftmost-outermost β-redex. Capture-avoiding substitution with fresh-name generation. `def`s inlined into `main`. Step-limited `normalize`.
*Learning:* α-equivalence, capture, why naive substitution is treacherous, leftmost-outermost as an algorithm.

**M3: REPL + stdlib loading.**
Read `lib/prelude.lc` on startup; persistent environment across REPL lines. Pretty-printer detects Church numerals, booleans, and pairs and prints them in human-readable form. `:load`, `:env`, `:quit` commands.
*Learning:* how REPLs handle parse errors; why pretty-printing matters; the joy of a working artifact.

**End of Phase A:** complete, correct, slow λ-calculus interpreter with working stdlib (including Y, factorial, list operations).

### Phase B — Real runtime

**M4: De Bruijn indices.**
Replace `Var(String)` with `Var(usize)` representing distance to binder. Substitution becomes index shifting. Round-trip AST → De Bruijn → AST. Benchmark vs M2.
*Learning:* names are overhead; the math reads cleaner without them; why every serious λ-calculus implementation uses De Bruijn or a variant.

**M5: Call-by-need (lazy with sharing).**
Replace eager substitution with environments and thunks. `M[x := N]` becomes a closure pairing `M` with an env binding `x ↦ N`. When `x` is demanded, `N` is reduced once and the result is cached. Graph reduction with `Rc<RefCell<Thunk>>` (or equivalent).
*Learning:* WHNF vs full normal form; thunks as the universal lazy primitive; how Haskell's runtime actually works; the dramatic speedup on Y-combinator-heavy programs.

**End of Phase B:** fast, lazy λ-calculus interpreter — a tiny Haskell.

### Phase C — Pick a rabbit hole

Milestones in this phase can be done in any order, or skipped, based on interest.

**M6: Optimization passes.**
Implement (in roughly this order, by reward-per-effort):
- η-reduction at parse/normalize time (`\x. f x` → `f` when `x` is not free in `f`).
- Inlining of small, non-recursive `def`s.
- Strictness analysis: identify arguments that will definitely be evaluated; skip thunk creation.
- Church-numeral primitive elision: detect Church-shaped terms, evaluate as native integers, re-inflate at boundaries.

*Learning:* compiler passes as program-to-program transformations; canonical optimization techniques in miniature.

**M7: SKI compilation.**
Bracket abstraction: a syntax-directed algorithm that compiles every λ-term into application of `S`, `K`, `I` only — no variables. Runtime is ~3 reduction rules.

*Learning:* compilation is just transformation; runtimes can be tiny if the input language is constrained; combinatory logic as an alternative foundation β-equivalent to λ-calculus.

**M8: An abstract machine.**
Implement either the **Krivine machine** (call-by-name, ~50 lines, perfect first abstract machine) or a **G-machine / spineless tagless G-machine variant** (call-by-need, much more involved, the basis of GHC). Stack-based, deterministic execution of compiled instructions.

*Learning:* how lazy languages compile down to efficient instruction-level execution; the gap between "interpreter" and "compiler" is smaller than it looks.

**End of Phase C:** the project has traversed the full spectrum from "tree-walking interpreter" to "compiler emitting bytecode for an abstract machine," touching every conceptual milestone in compiler construction along the way except native code emission.

## 6. Out of Scope

The following are explicitly **not** part of this design and are listed only as possible follow-on projects:

- **Native code emission.** Would require closure conversion, a thunk runtime in low-level IR, a garbage collector, and either an LLVM/Cranelift integration or direct machine-code output. Easily as much work as M0–M8 combined. Adds little new *theory* beyond what Phase C already covers; mostly engineering. Not pursued.
- **A type system** (Hindley-Milner or System F). The language is untyped by deliberate choice. Adding types would be a large, separate project that elaborates a typed surface language down to the existing untyped core.
- **An optimizer DSL.** Express M6 optimizations as rewrite rules processed by a generic engine. Interesting; out of scope for this design.
- **Surface-syntax extensions.** Multi-argument λ-sugar (`\x y. M`), `let` expressions, integer literals, pattern matching, modules. All deferred. The current syntax is intentionally minimal.

## 7. Testing Strategy

- **Unit tests** for each module: lexer/parser round-trip, substitution correctness, single-step reduction, capture cases.
- **Integration tests** as `(source_string, expected_normal_form_string)` pairs covering each tier of the stdlib.
- **The stdlib itself is the largest test corpus.** Every encoding has predicted reduction behavior (e.g., `succ (succ (succ zero))` reduces to the same normal form as `three`); these equivalences become assertions.
- **Property-style tests** at later milestones: De Bruijn round-trip preserves semantics (M4); call-by-need produces the same normal forms as call-by-name (M5); SKI compilation preserves semantics (M7).
- **Benchmark harness** at M4 onward: factorial, Fibonacci, naive list traversals — used to validate that each optimization milestone produces measurable speedups.

## 8. Open Questions

None at design time. Implementation-level questions (specific Rust idioms for the graph reducer, the exact bracket-abstraction algorithm to use, etc.) will be resolved in the implementation plan.
