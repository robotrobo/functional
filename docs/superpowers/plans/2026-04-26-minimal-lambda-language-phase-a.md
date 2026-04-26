# Minimal Lambda-Calculus Language — Phase A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working call-by-name normal-order interpreter for pure untyped lambda calculus in Rust, with a stdlib (booleans, numerals, pairs, Y, lists) written in the language itself, accessible via both file mode and REPL.

**Architecture:** Single Rust binary crate. `chumsky`-based parser produces an `Expr` AST (`Var | Abs | App`) plus top-level `def`s. A tree-walking evaluator performs leftmost-outermost reduction with capture-avoiding substitution. `def`s are inlined into the main expression before reduction. The REPL uses `rustyline` for line editing and a Church-shape detector in the pretty-printer to display `(succ (succ (succ zero)))` as `3`.

**Tech Stack:** Rust 2021 edition, `chumsky` 0.9 (parser), `rustyline` 14 (REPL), `thiserror` (error types), `anyhow` (top-level error glue).

**Spec reference:** [`docs/superpowers/specs/2026-04-26-minimal-lambda-language-design.md`](../specs/2026-04-26-minimal-lambda-language-design.md)

---

## File Structure

```
Cargo.toml
src/
  main.rs        -- CLI entry: file mode | repl mode
  lib.rs         -- crate root, re-exports
  ast.rs         -- Expr, Program, Def, Spanned
  parser.rs      -- chumsky parser → Program
  pretty.rs      -- Expr → String, with Church-shape detection
  eval.rs        -- substitution, reduce_step, normalize
  repl.rs        -- interactive loop using rustyline
  error.rs       -- ParseError, EvalError types
lib/
  prelude.lc     -- the standard library, written in the language itself
tests/
  parser_test.rs -- parser round-trip & negative cases
  eval_test.rs   -- reduction unit tests (capture, redex finding, etc.)
  stdlib_test.rs -- end-to-end: load prelude.lc, evaluate sample programs
examples/
  identity.lc
  bool_demo.lc
  fact.lc
docs/superpowers/
  specs/
  plans/
```

Each file has one responsibility; no file should exceed ~300 lines through Phase A. If `eval.rs` grows past that, split substitution into `subst.rs`.

---

## Milestone 0 — Bootstrap & Hello AST

End state: `cargo run` prints the result of reducing a hand-built AST `((\x. x) (\y. y))` and shows `\y. y`.

### Task 1: Initialize project + git + dependencies

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `src/main.rs`, `src/lib.rs`

- [ ] **Step 1: Initialize cargo + git**

```bash
cd /Users/anishagrawal/Code/fun/functional
cargo init --name lc --vcs git
```

Expected: creates `Cargo.toml`, `src/main.rs`, `.gitignore`, initializes git.

- [ ] **Step 2: Add dependencies to `Cargo.toml`**

Replace `[dependencies]` (or add) so the file matches:

```toml
[package]
name = "lc"
version = "0.1.0"
edition = "2021"

[dependencies]
chumsky = "0.9"
rustyline = "14"
thiserror = "1"
anyhow = "1"

[lib]
name = "lc"
path = "src/lib.rs"
```

- [ ] **Step 3: Create stub `src/lib.rs`**

```rust
pub mod ast;
pub mod parser;
pub mod pretty;
pub mod eval;
pub mod error;
pub mod repl;
```

(Modules will be filled in by later tasks; for now create empty `src/ast.rs`, `src/parser.rs`, `src/pretty.rs`, `src/eval.rs`, `src/error.rs`, `src/repl.rs` so the crate compiles.)

```bash
touch src/ast.rs src/parser.rs src/pretty.rs src/eval.rs src/error.rs src/repl.rs
```

- [ ] **Step 4: Verify it builds**

```bash
cargo build
```

Expected: clean build, only warnings about unused empty modules.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: scaffold cargo project with chumsky and rustyline deps"
```

---

### Task 2: Core AST types

**Files:**
- Modify: `src/ast.rs`

- [ ] **Step 1: Write a failing test for AST construction and equality**

Append to `src/ast.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_identity_lambda() {
        let e = Expr::abs("x", Expr::var("x"));
        assert_eq!(
            e,
            Expr::Abs("x".to_string(), Box::new(Expr::Var("x".to_string())))
        );
    }

    #[test]
    fn build_application() {
        let e = Expr::app(Expr::var("f"), Expr::var("x"));
        assert_eq!(
            e,
            Expr::App(
                Box::new(Expr::Var("f".to_string())),
                Box::new(Expr::Var("x".to_string()))
            )
        );
    }

    #[test]
    fn program_with_one_def() {
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: None,
        };
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.defs[0].name, "id");
    }
}
```

- [ ] **Step 2: Run test to verify failure**

```bash
cargo test --lib ast::tests
```

Expected: compile errors — `Expr`, `Program`, `Def`, `Expr::var`, `Expr::abs`, `Expr::app` not defined.

- [ ] **Step 3: Implement AST types**

Prepend to `src/ast.rs` (above the test module):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Var(String),
    Abs(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Var(name.into())
    }

    pub fn abs(param: impl Into<String>, body: Expr) -> Self {
        Expr::Abs(param.into(), Box::new(body))
    }

    pub fn app(f: Expr, x: Expr) -> Self {
        Expr::App(Box::new(f), Box::new(x))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub body: Expr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub defs: Vec<Def>,
    pub main: Option<Expr>,
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib ast::tests
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 5: Commit**

```bash
git add src/ast.rs
git commit -m "feat(ast): define Expr, Def, Program types and constructors"
```

---

### Task 3: Pretty-printer for AST

**Files:**
- Modify: `src/pretty.rs`

- [ ] **Step 1: Write failing tests covering precedence and parenthesization**

Replace `src/pretty.rs` with:

```rust
use crate::ast::Expr;

pub fn print(e: &Expr) -> String {
    print_expr(e)
}

fn print_expr(e: &Expr) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    #[test]
    fn print_var() {
        assert_eq!(print(&Expr::var("x")), "x");
    }

    #[test]
    fn print_simple_lambda() {
        assert_eq!(print(&Expr::abs("x", Expr::var("x"))), "\\x. x");
    }

    #[test]
    fn print_application_left_assoc() {
        // f x y is ((f x) y) — should print without redundant parens
        let e = Expr::app(
            Expr::app(Expr::var("f"), Expr::var("x")),
            Expr::var("y"),
        );
        assert_eq!(print(&e), "f x y");
    }

    #[test]
    fn print_application_with_lambda_argument_is_parenthesized() {
        // f (\x. x) — lambda must be parenthesized when it's an argument
        let e = Expr::app(Expr::var("f"), Expr::abs("x", Expr::var("x")));
        assert_eq!(print(&e), "f (\\x. x)");
    }

    #[test]
    fn print_lambda_body_extends_right() {
        // \x. f x y — body is the entire (f x y), no parens needed around body
        let e = Expr::abs(
            "x",
            Expr::app(
                Expr::app(Expr::var("f"), Expr::var("x")),
                Expr::var("y"),
            ),
        );
        assert_eq!(print(&e), "\\x. f x y");
    }

    #[test]
    fn print_nested_application_lhs_parens() {
        // (\x. x) y — lambda on the left of an application must be parenthesized
        let e = Expr::app(Expr::abs("x", Expr::var("x")), Expr::var("y"));
        assert_eq!(print(&e), "(\\x. x) y");
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test --lib pretty
```

Expected: `todo!()` panic on first test.

- [ ] **Step 3: Implement the pretty-printer**

Replace the `print_expr` body in `src/pretty.rs`:

```rust
fn print_expr(e: &Expr) -> String {
    match e {
        Expr::Var(name) => name.clone(),
        Expr::Abs(param, body) => format!("\\{}. {}", param, print_expr(body)),
        Expr::App(f, x) => {
            let f_str = match **f {
                Expr::Abs(_, _) => format!("({})", print_expr(f)),
                _ => print_expr(f),
            };
            let x_str = match **x {
                Expr::App(_, _) | Expr::Abs(_, _) => format!("({})", print_expr(x)),
                _ => print_expr(x),
            };
            format!("{} {}", f_str, x_str)
        }
    }
}
```

Why these rules:
- A lambda on the **left** of an application needs parens (otherwise it parses as a lambda whose body is the application).
- A lambda or application on the **right** of an application needs parens (because application is left-associative and lambda bodies extend right).
- A lambda body never needs outer parens; it just extends rightward.

- [ ] **Step 4: Run tests**

```bash
cargo test --lib pretty
```

Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/pretty.rs
git commit -m "feat(pretty): print Expr to source-form string with correct paren rules"
```

---

### Task 4: Hand-coded β-reduction (M0 milestone artifact)

This task proves end-to-end that we can build an AST and reduce it. Substitution here is the **naive, capture-unsafe** version — Task 9 will replace it with the capture-avoiding version. We only need correctness on simple cases that don't trigger capture.

**Files:**
- Modify: `src/eval.rs`, `src/main.rs`

- [ ] **Step 1: Write failing test for naive substitution + identity reduction**

Replace `src/eval.rs` with:

```rust
use crate::ast::Expr;

/// **NAIVE** substitution. Will be replaced in Task 9 with capture-avoiding
/// substitution. Correct only on terms where no capture is possible.
pub fn naive_subst(target: &Expr, x: &str, value: &Expr) -> Expr {
    match target {
        Expr::Var(name) if name == x => value.clone(),
        Expr::Var(name) => Expr::Var(name.clone()),
        Expr::Abs(param, _) if param == x => target.clone(),
        Expr::Abs(param, body) => {
            Expr::abs(param.clone(), naive_subst(body, x, value))
        }
        Expr::App(f, a) => Expr::app(
            naive_subst(f, x, value),
            naive_subst(a, x, value),
        ),
    }
}

/// One step of leftmost-outermost reduction. Naive — no capture handling yet.
pub fn naive_reduce_step(e: &Expr) -> Option<Expr> {
    match e {
        Expr::App(f, a) => {
            if let Expr::Abs(param, body) = &**f {
                Some(naive_subst(body, param, a))
            } else if let Some(f2) = naive_reduce_step(f) {
                Some(Expr::app(f2, (**a).clone()))
            } else {
                naive_reduce_step(a).map(|a2| Expr::app((**f).clone(), a2))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    #[test]
    fn identity_applied_to_identity() {
        // (\x. x) (\y. y)  →  \y. y
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let stepped = naive_reduce_step(&e).unwrap();
        assert_eq!(stepped, Expr::abs("y", Expr::var("y")));
    }

    #[test]
    fn no_redex_returns_none() {
        let e = Expr::abs("x", Expr::var("x"));
        assert!(naive_reduce_step(&e).is_none());
    }
}
```

- [ ] **Step 2: Run tests to confirm failure (compile errors only — module empty)**

```bash
cargo test --lib eval
```

Expected: tests run after edits — both pass once code is in place. (If you copy the code above as-is in this step, the tests will pass; that's fine. The "failing first" pattern here was conceptual — there was no prior code.)

- [ ] **Step 3: Wire `main.rs` to demonstrate end-to-end reduction**

Replace `src/main.rs` with:

```rust
use lc::ast::Expr;
use lc::eval::naive_reduce_step;
use lc::pretty::print;

fn main() {
    // ((\x. x) (\y. y))  should reduce to (\y. y)
    let term = Expr::app(
        Expr::abs("x", Expr::var("x")),
        Expr::abs("y", Expr::var("y")),
    );
    println!("input:  {}", print(&term));
    let reduced = naive_reduce_step(&term).expect("expected one redex");
    println!("output: {}", print(&reduced));
}
```

- [ ] **Step 4: Run the binary**

```bash
cargo run
```

Expected output:

```
input:  (\x. x) (\y. y)
output: \y. y
```

- [ ] **Step 5: Commit**

```bash
git add src/eval.rs src/main.rs
git commit -m "feat(eval): naive β-reduction proof of concept; main demonstrates identity application"
```

---

**End of M0.** You have a working AST, pretty-printer, and a correct-on-easy-cases reducer. Onward to parsing.

---

## Milestone 1 — Parser via `chumsky`

End state: `cargo run -- examples/identity.lc` parses a file, pretty-prints the AST, and round-trips correctly.

### Task 5: Error type for parse failures

**Files:**
- Modify: `src/error.rs`

- [ ] **Step 1: Define the error types**

Replace `src/error.rs` with:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Generic(String),
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("free variable referenced: {0}")]
    FreeVariable(String),

    #[error("reduction step limit ({0}) exceeded")]
    StepLimitExceeded(usize),
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build
```

Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "feat(error): add ParseError and EvalError types"
```

---

### Task 6: Parse a single identifier

**Files:**
- Modify: `src/parser.rs`

- [ ] **Step 1: Write a failing test**

Replace `src/parser.rs` with:

```rust
use chumsky::prelude::*;

use crate::ast::Expr;
use crate::error::ParseError;

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let _ = src;
    Err(ParseError::Generic("unimplemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_identifier() {
        assert_eq!(parse_expr("x").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_identifier_with_whitespace() {
        assert_eq!(parse_expr("  x  ").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_underscore_identifier() {
        assert_eq!(parse_expr("_foo").unwrap(), Expr::var("_foo"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib parser
```

Expected: all 3 tests fail with "unimplemented".

- [ ] **Step 3: Implement minimal parser using chumsky**

Replace `src/parser.rs`:

```rust
use chumsky::prelude::*;

use crate::ast::Expr;
use crate::error::ParseError;

fn ident() -> impl Parser<char, String, Error = Simple<char>> {
    filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .map(|c| c.to_string())
        .chain::<char, _, _>(
            filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_').repeated(),
        )
        .collect::<String>()
        .padded()
}

fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    ident().map(Expr::Var)
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    expr_parser()
        .then_ignore(end())
        .parse(src)
        .map_err(|errs| {
            ParseError::Generic(
                errs.into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_identifier() {
        assert_eq!(parse_expr("x").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_identifier_with_whitespace() {
        assert_eq!(parse_expr("  x  ").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_underscore_identifier() {
        assert_eq!(parse_expr("_foo").unwrap(), Expr::var("_foo"));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib parser
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/parser.rs
git commit -m "feat(parser): parse identifiers"
```

---

### Task 7: Parse lambda abstractions and applications

**Files:**
- Modify: `src/parser.rs`

- [ ] **Step 1: Append failing tests**

Add to the `tests` module in `src/parser.rs` (keep existing tests):

```rust
    #[test]
    fn parse_lambda_identity() {
        assert_eq!(
            parse_expr("\\x. x").unwrap(),
            Expr::abs("x", Expr::var("x"))
        );
    }

    #[test]
    fn parse_application_left_assoc() {
        // f x y  parses as ((f x) y)
        assert_eq!(
            parse_expr("f x y").unwrap(),
            Expr::app(
                Expr::app(Expr::var("f"), Expr::var("x")),
                Expr::var("y"),
            )
        );
    }

    #[test]
    fn parse_lambda_body_extends_right() {
        // \x. f x  is  \x. (f x), not (\x. f) x
        assert_eq!(
            parse_expr("\\x. f x").unwrap(),
            Expr::abs("x", Expr::app(Expr::var("f"), Expr::var("x")))
        );
    }

    #[test]
    fn parse_parenthesized_application() {
        assert_eq!(
            parse_expr("(\\x. x) y").unwrap(),
            Expr::app(
                Expr::abs("x", Expr::var("x")),
                Expr::var("y"),
            )
        );
    }

    #[test]
    fn parse_y_combinator() {
        // \f. (\x. f (x x)) (\x. f (x x))
        let inner = Expr::abs(
            "x",
            Expr::app(
                Expr::var("f"),
                Expr::app(Expr::var("x"), Expr::var("x")),
            ),
        );
        let expected = Expr::abs(
            "f",
            Expr::app(inner.clone(), inner),
        );
        assert_eq!(
            parse_expr("\\f. (\\x. f (x x)) (\\x. f (x x))").unwrap(),
            expected
        );
    }
```

- [ ] **Step 2: Run tests to confirm failures**

```bash
cargo test --lib parser
```

Expected: 5 new tests fail (lambda/application not implemented).

- [ ] **Step 3: Replace `expr_parser` with the full grammar**

Replace `expr_parser()` in `src/parser.rs`:

```rust
fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr| {
        let var = ident().map(Expr::Var);

        let lambda = just('\\')
            .ignore_then(ident())
            .then_ignore(just('.').padded())
            .then(expr.clone())
            .map(|(param, body)| Expr::abs(param, body));

        let parens = expr
            .clone()
            .delimited_by(just('(').padded(), just(')').padded());

        let atom = choice((lambda, parens, var)).padded();

        atom.repeated()
            .at_least(1)
            .map(|atoms| {
                let mut iter = atoms.into_iter();
                let head = iter.next().unwrap();
                iter.fold(head, Expr::app)
            })
    })
}
```

Note the order in `choice((lambda, parens, var))` matters: lambda must come before var so `\x. ...` is not parsed as a variable named `\` followed by garbage. `parens` before `var` is harmless but conventional.

- [ ] **Step 4: Run tests**

```bash
cargo test --lib parser
```

Expected: all parser tests pass (3 original + 5 new = 8).

- [ ] **Step 5: Commit**

```bash
git add src/parser.rs
git commit -m "feat(parser): parse lambdas and left-associative applications"
```

---

### Task 8: Parse `def`s and full programs

**Files:**
- Modify: `src/parser.rs`

- [ ] **Step 1: Append failing tests**

Add to the `tests` module in `src/parser.rs`:

```rust
    use crate::ast::{Def, Program};

    fn parse_program(src: &str) -> Result<Program, ParseError> {
        super::parse_program(src)
    }

    #[test]
    fn parse_empty_program() {
        let p = parse_program("").unwrap();
        assert!(p.defs.is_empty());
        assert!(p.main.is_none());
    }

    #[test]
    fn parse_program_with_one_def_and_main() {
        let p = parse_program("def id = \\x. x\nid x").unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.defs[0].name, "id");
        assert_eq!(p.defs[0].body, Expr::abs("x", Expr::var("x")));
        assert_eq!(
            p.main.as_ref().unwrap(),
            &Expr::app(Expr::var("id"), Expr::var("x"))
        );
    }

    #[test]
    fn parse_program_with_comments() {
        let src = "-- this is a comment\n\
                   def id = \\x. x  -- inline comment\n\
                   id";
        let p = parse_program(src).unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.main.as_ref().unwrap(), &Expr::var("id"));
    }
```

- [ ] **Step 2: Run tests to confirm failures**

```bash
cargo test --lib parser
```

Expected: 3 new tests fail; symbol `parse_program` not found.

- [ ] **Step 3: Implement comment handling and program parser**

The simplest robust approach is to strip line comments **before** parsing. `chumsky`'s `.padded()` only skips whitespace, not `--`-style comments, so we preprocess the source.

Add to `src/parser.rs` (keep what's already there):

```rust
use crate::ast::{Def, Program};

/// Strip `-- ...` to end-of-line comments. Operates line by line so byte
/// offsets shift uniformly per line; close enough for our purposes.
fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn def_parser() -> impl Parser<char, Def, Error = Simple<char>> {
    text::keyword("def")
        .padded()
        .ignore_then(ident())
        .then_ignore(just('=').padded())
        .then(expr_parser())
        .map(|(name, body)| Def { name, body })
}

fn program_parser() -> impl Parser<char, Program, Error = Simple<char>> {
    def_parser()
        .padded()
        .repeated()
        .then(expr_parser().or_not())
        .then_ignore(end())
        .map(|(defs, main)| Program { defs, main })
}

pub fn parse_program(src: &str) -> Result<Program, ParseError> {
    let cleaned = strip_comments(src);
    program_parser().parse(cleaned.as_str()).map_err(|errs| {
        ParseError::Generic(
            errs.into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}
```

This is uglier than handling comments in the grammar, but it's correct, takes 8 lines, and is easy to swap out for a proper grammar-level skipper later if we ever need preserved spans.

- [ ] **Step 4: Run tests**

```bash
cargo test --lib parser
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/parser.rs
git commit -m "feat(parser): parse defs and programs with comments"
```

---

### Task 9: Parser round-trip integration test + example file

**Files:**
- Create: `examples/identity.lc`, `tests/parser_test.rs`

- [ ] **Step 1: Create an example program**

Write `examples/identity.lc`:

```
def id = \x. x
def const = \x. \y. x

const (id apple) banana
```

- [ ] **Step 2: Create round-trip test**

Write `tests/parser_test.rs`:

```rust
use lc::parser::parse_program;
use lc::pretty::print;

#[test]
fn round_trip_examples_identity() {
    let src = std::fs::read_to_string("examples/identity.lc").unwrap();
    let parsed = parse_program(&src).expect("parse should succeed");
    assert_eq!(parsed.defs.len(), 2);
    let main = parsed.main.expect("main expected");
    let printed = print(&main);
    assert_eq!(printed, "const (id apple) banana");
}
```

- [ ] **Step 3: Run the integration test**

```bash
cargo test --test parser_test
```

Expected: 1 test passes.

- [ ] **Step 4: Update `main.rs` to read a file from CLI args**

Replace `src/main.rs`:

```rust
use std::env;
use std::process;

use lc::parser::parse_program;
use lc::pretty::print;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: lc <file.lc>");
        process::exit(2);
    }
    let path = &args[1];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            process::exit(1);
        }
    };
    match parse_program(&src) {
        Ok(p) => {
            for d in &p.defs {
                println!("def {} = {}", d.name, print(&d.body));
            }
            if let Some(m) = &p.main {
                println!("main = {}", print(m));
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
```

- [ ] **Step 5: Run end-to-end**

```bash
cargo run -- examples/identity.lc
```

Expected output:

```
def id = \x. x
def const = \x. \y. x
main = const (id apple) banana
```

- [ ] **Step 6: Commit**

```bash
git add examples/identity.lc tests/parser_test.rs src/main.rs
git commit -m "feat: end-to-end parse + pretty-print pipeline; round-trip integration test"
```

---

**End of M1.** Parser, pretty-printer, file mode CLI all working. Time for the real evaluator.

---

## Milestone 2 — Tree-walking evaluator

End state: `cargo run -- examples/fact.lc` evaluates factorial of small numbers using the Y combinator. Capture-avoiding substitution; leftmost-outermost reduction; defs inlined.

### Task 10: Free variable computation

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Add a failing test for `free_vars`**

Append to `src/eval.rs` (inside the `tests` module):

```rust
    use std::collections::BTreeSet;

    fn fv_set(e: &Expr) -> BTreeSet<String> {
        super::free_vars(e).into_iter().collect()
    }

    #[test]
    fn free_var_of_variable() {
        assert_eq!(fv_set(&Expr::var("x")), ["x"].iter().map(|s| s.to_string()).collect());
    }

    #[test]
    fn free_var_of_lambda_excludes_param() {
        // \x. x has no free vars; \x. y has {y}
        assert_eq!(fv_set(&Expr::abs("x", Expr::var("x"))), BTreeSet::new());
        let yfree = Expr::abs("x", Expr::var("y"));
        assert_eq!(fv_set(&yfree), ["y"].iter().map(|s| s.to_string()).collect());
    }

    #[test]
    fn free_var_of_app() {
        let e = Expr::app(Expr::var("f"), Expr::var("x"));
        assert_eq!(
            fv_set(&e),
            ["f", "x"].iter().map(|s| s.to_string()).collect()
        );
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib eval
```

Expected: compile error — `free_vars` not defined.

- [ ] **Step 3: Implement `free_vars`**

Add to `src/eval.rs` (above the `tests` module):

```rust
use std::collections::HashSet;

pub fn free_vars(e: &Expr) -> HashSet<String> {
    let mut acc = HashSet::new();
    free_vars_into(e, &mut acc);
    acc
}

fn free_vars_into(e: &Expr, acc: &mut HashSet<String>) {
    match e {
        Expr::Var(name) => {
            acc.insert(name.clone());
        }
        Expr::Abs(param, body) => {
            let mut inner = HashSet::new();
            free_vars_into(body, &mut inner);
            inner.remove(param);
            acc.extend(inner);
        }
        Expr::App(f, x) => {
            free_vars_into(f, acc);
            free_vars_into(x, acc);
        }
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib eval
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/eval.rs
git commit -m "feat(eval): compute free variables of an expression"
```

---

### Task 11: Capture-avoiding substitution

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Add the canonical capture test**

Append to `tests` module in `src/eval.rs`:

```rust
    #[test]
    fn substitution_avoids_capture() {
        // (\y. x)[x := y]  must NOT yield (\y. y)
        // Correct: alpha-rename the inner y, e.g. to (\y'. y) — equivalently any fresh name.
        let target = Expr::abs("y", Expr::var("x"));
        let value = Expr::var("y");
        let result = subst(&target, "x", &value);

        // Result must be of the form \<fresh>. y where <fresh> != "y"
        match result {
            Expr::Abs(param, body) => {
                assert_ne!(param, "y", "must α-rename to avoid capture");
                assert_eq!(*body, Expr::var("y"), "body should be the substituted value");
            }
            other => panic!("expected an Abs, got {:?}", other),
        }
    }

    #[test]
    fn substitution_into_variable() {
        let target = Expr::var("x");
        let value = Expr::var("y");
        assert_eq!(subst(&target, "x", &value), Expr::var("y"));
    }

    #[test]
    fn substitution_does_not_descend_into_shadowing_lambda() {
        // (\x. x)[x := y]  =  \x. x   (the inner x is not free)
        let target = Expr::abs("x", Expr::var("x"));
        let value = Expr::var("y");
        assert_eq!(subst(&target, "x", &value), target);
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib eval
```

Expected: compile error — `subst` not defined.

- [ ] **Step 3: Implement capture-avoiding substitution**

Add to `src/eval.rs`:

```rust
/// Generate a fresh name not in `taken`. We append apostrophes; if `name` is
/// already taken we add `'` until we find one that isn't.
fn fresh_name(name: &str, taken: &HashSet<String>) -> String {
    let mut candidate = format!("{}'", name);
    while taken.contains(&candidate) {
        candidate.push('\'');
    }
    candidate
}

/// Capture-avoiding substitution: target[x := value].
pub fn subst(target: &Expr, x: &str, value: &Expr) -> Expr {
    match target {
        Expr::Var(name) if name == x => value.clone(),
        Expr::Var(_) => target.clone(),

        // The bound variable shadows x: no substitution into the body.
        Expr::Abs(param, _) if param == x => target.clone(),

        Expr::Abs(param, body) => {
            let value_fvs = free_vars(value);
            if value_fvs.contains(param) {
                // Capture would occur. α-rename `param` to a fresh name.
                let mut taken = value_fvs.clone();
                taken.extend(free_vars(body));
                taken.insert(x.to_string());
                let new_param = fresh_name(param, &taken);
                let renamed_body = subst(body, param, &Expr::var(&new_param));
                Expr::abs(new_param, subst(&renamed_body, x, value))
            } else {
                Expr::abs(param.clone(), subst(body, x, value))
            }
        }

        Expr::App(f, a) => Expr::app(subst(f, x, value), subst(a, x, value)),
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib eval
```

Expected: all eval tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/eval.rs
git commit -m "feat(eval): capture-avoiding substitution with fresh-name α-renaming"
```

---

### Task 12: Leftmost-outermost reduce_step using safe `subst`

**Files:**
- Modify: `src/eval.rs`

- [ ] **Step 1: Add tests for the real reducer**

Append to `tests` module:

```rust
    #[test]
    fn reduce_step_finds_leftmost_outermost_redex() {
        // ((\x. x) (\y. y)) z  →  (\y. y) z   (outer redex first, not inner)
        let inner = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let e = Expr::app(inner, Expr::var("z"));
        let stepped = reduce_step(&e).unwrap();
        // After one step: (\y. y) z
        assert_eq!(
            stepped,
            Expr::app(Expr::abs("y", Expr::var("y")), Expr::var("z"))
        );
    }

    #[test]
    fn reduce_step_capture_safe() {
        // Trigger capture: (\x. \y. x) y  must α-rename to give \<fresh>. y
        let e = Expr::app(
            Expr::abs("x", Expr::abs("y", Expr::var("x"))),
            Expr::var("y"),
        );
        let stepped = reduce_step(&e).unwrap();
        match stepped {
            Expr::Abs(param, body) => {
                assert_ne!(param, "y");
                assert_eq!(*body, Expr::var("y"));
            }
            other => panic!("expected Abs, got {:?}", other),
        }
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib eval
```

Expected: `reduce_step` not defined.

- [ ] **Step 3: Implement `reduce_step`**

Add to `src/eval.rs`:

```rust
/// One step of leftmost-outermost (call-by-name normal-order) reduction.
/// Returns None if the term is in normal form.
pub fn reduce_step(e: &Expr) -> Option<Expr> {
    match e {
        Expr::App(f, a) => {
            // 1. If the head is a lambda, β-reduce here (outermost).
            if let Expr::Abs(param, body) = f.as_ref() {
                return Some(subst(body, param, a));
            }
            // 2. Otherwise try to reduce in the function position (leftmost).
            if let Some(f2) = reduce_step(f) {
                return Some(Expr::app(f2, a.as_ref().clone()));
            }
            // 3. Then in the argument position.
            //    Note: under STRICT normal-order we'd descend here only
            //    after the head is in head normal form; that's exactly what
            //    we just did.
            reduce_step(a).map(|a2| Expr::app(f.as_ref().clone(), a2))
        }
        Expr::Abs(_, _) => {
            // We DO NOT reduce inside lambda bodies during execution
            // (weak head normal form). Section 2.3 of the spec.
            None
        }
        Expr::Var(_) => None,
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib eval
```

Expected: all eval tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/eval.rs
git commit -m "feat(eval): leftmost-outermost reduce_step with weak head normal form"
```

---

### Task 13: `normalize` driver with step limit; remove `naive_*` versions

**Files:**
- Modify: `src/eval.rs`, `src/main.rs`

- [ ] **Step 1: Add tests for `normalize`**

Append to `tests` module:

```rust
    use crate::error::EvalError;

    #[test]
    fn normalize_terminating_reaches_normal_form() {
        // (\x. x) (\y. y)  → \y. y
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let nf = normalize(e, 1000).unwrap();
        assert_eq!(nf, Expr::abs("y", Expr::var("y")));
    }

    #[test]
    fn normalize_step_limit_exceeded() {
        // Ω = (\x. x x)(\x. x x)  diverges
        let omega_term = Expr::abs(
            "x",
            Expr::app(Expr::var("x"), Expr::var("x")),
        );
        let omega = Expr::app(omega_term.clone(), omega_term);
        let res = normalize(omega, 50);
        match res {
            Err(EvalError::StepLimitExceeded(n)) => assert_eq!(n, 50),
            other => panic!("expected StepLimitExceeded, got {:?}", other),
        }
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib eval
```

Expected: `normalize` not defined.

- [ ] **Step 3: Implement `normalize` and remove the naive helpers**

In `src/eval.rs`, **delete** `naive_subst` and `naive_reduce_step` (and their tests), then add:

```rust
use crate::error::EvalError;

/// Reduce to weak head normal form, with a step limit.
pub fn normalize(mut e: Expr, step_limit: usize) -> Result<Expr, EvalError> {
    for _ in 0..step_limit {
        match reduce_step(&e) {
            Some(next) => e = next,
            None => return Ok(e),
        }
    }
    Err(EvalError::StepLimitExceeded(step_limit))
}
```

Also delete the old `identity_applied_to_identity` and `no_redex_returns_none` tests if they reference `naive_*` — replace with the new `normalize_*` tests above (already added).

- [ ] **Step 4: Update `main.rs` to use `normalize`**

Replace `src/main.rs`:

```rust
use std::env;
use std::process;

use lc::eval::normalize;
use lc::parser::parse_program;
use lc::pretty::print;

const DEFAULT_STEP_LIMIT: usize = 100_000;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: lc <file.lc>");
        process::exit(2);
    }
    let path = &args[1];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            process::exit(1);
        }
    };
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    if let Some(main_expr) = program.main {
        // M2: defs are inlined into main (Task 14).
        // For now (no inlining yet), just normalize main as-is.
        match normalize(main_expr, DEFAULT_STEP_LIMIT) {
            Ok(nf) => println!("{}", print(&nf)),
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    } else {
        for d in &program.defs {
            println!("def {} = {}", d.name, print(&d.body));
        }
    }
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all unit + integration tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/eval.rs src/main.rs
git commit -m "feat(eval): normalize driver with step limit; remove naive helpers"
```

---

### Task 14: Inline `def`s into main expression

**Files:**
- Modify: `src/eval.rs`, `src/main.rs`

- [ ] **Step 1: Add a test for inlining**

Append to `tests` module:

```rust
    use crate::ast::{Def, Program};

    #[test]
    fn inline_defs_into_main() {
        // def id = \x. x ; main = id y  →  (\x. x) y
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: Some(Expr::app(Expr::var("id"), Expr::var("y"))),
        };
        let inlined = inline_defs(&p).unwrap();
        assert_eq!(
            inlined,
            Expr::app(
                Expr::abs("x", Expr::var("x")),
                Expr::var("y"),
            )
        );
    }

    #[test]
    fn inline_chained_defs() {
        // def a = \x. x ; def b = a ; main = b  →  \x. x
        let p = Program {
            defs: vec![
                Def { name: "a".into(), body: Expr::abs("x", Expr::var("x")) },
                Def { name: "b".into(), body: Expr::var("a") },
            ],
            main: Some(Expr::var("b")),
        };
        let inlined = inline_defs(&p).unwrap();
        assert_eq!(inlined, Expr::abs("x", Expr::var("x")));
    }

    #[test]
    fn inline_missing_def_yields_free_variable_error() {
        let p = Program {
            defs: vec![],
            main: Some(Expr::var("oops")),
        };
        match inline_defs(&p) {
            Err(EvalError::FreeVariable(name)) => assert_eq!(name, "oops"),
            other => panic!("expected FreeVariable, got {:?}", other),
        }
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib eval
```

Expected: `inline_defs` not defined.

- [ ] **Step 3: Implement `inline_defs`**

Add to `src/eval.rs`:

```rust
use crate::ast::{Def, Program};

/// Inline all `def`s into `main`. Each def is also inlined into subsequent
/// defs, so def order matters (no forward references). The result is a
/// closed term ready to normalize, or a FreeVariable error if any `Var`
/// references a name that is neither bound by a lambda nor defined.
pub fn inline_defs(p: &Program) -> Result<Expr, EvalError> {
    let main = p
        .main
        .clone()
        .ok_or_else(|| EvalError::FreeVariable("<no main expression>".into()))?;

    // Substitute each def's body for its name, in dependency order.
    // First, resolve cross-def references: rebuild defs so each body
    // already has previous defs inlined into it.
    let mut resolved: Vec<Def> = Vec::with_capacity(p.defs.len());
    for d in &p.defs {
        let mut body = d.body.clone();
        for prior in &resolved {
            body = subst(&body, &prior.name, &prior.body);
        }
        resolved.push(Def { name: d.name.clone(), body });
    }

    // Now inline into main.
    let mut result = main;
    for d in &resolved {
        result = subst(&result, &d.name, &d.body);
    }

    // Verify there are no remaining free variables.
    let remaining = free_vars(&result);
    if let Some(name) = remaining.into_iter().next() {
        return Err(EvalError::FreeVariable(name));
    }
    Ok(result)
}
```

- [ ] **Step 4: Wire `main.rs` to call `inline_defs`**

In `src/main.rs`, replace the body of the `if let Some(main_expr) = program.main` block:

```rust
    let inlined = match lc::eval::inline_defs(&program) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    match normalize(inlined, DEFAULT_STEP_LIMIT) {
        Ok(nf) => println!("{}", print(&nf)),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
```

(Adjust the surrounding `if let` so it pulls out only what's needed; the `program.main` check is now inside `inline_defs`.)

A cleaner version of the same block:

```rust
    if program.main.is_none() {
        // Library file with only defs — print them and exit.
        for d in &program.defs {
            println!("def {} = {}", d.name, print(&d.body));
        }
        return;
    }
    let inlined = match lc::eval::inline_defs(&program) {
        Ok(e) => e,
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    };
    match normalize(inlined, DEFAULT_STEP_LIMIT) {
        Ok(nf) => println!("{}", print(&nf)),
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    }
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/eval.rs src/main.rs
git commit -m "feat(eval): inline defs into main with cross-def substitution"
```

---

### Task 15: End-to-end test — booleans and small numerals via examples/

**Files:**
- Create: `examples/bool_demo.lc`, `tests/eval_test.rs`

- [ ] **Step 1: Create example file**

Write `examples/bool_demo.lc`:

```
def true  = \t. \f. t
def false = \t. \f. f
def not   = \p. p false true

not true
```

- [ ] **Step 2: Create integration test**

Write `tests/eval_test.rs`:

```rust
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

fn run(path: &str) -> String {
    let src = std::fs::read_to_string(path).expect("file");
    let p = parse_program(&src).expect("parse");
    let inlined = inline_defs(&p).expect("inline");
    let nf = normalize(inlined, 100_000).expect("normalize");
    print(&nf)
}

#[test]
fn not_true_reduces_to_false_form() {
    // not true =β false = \t. \f. f
    let result = run("examples/bool_demo.lc");
    assert_eq!(result, "\\t. \\f. f");
}
```

- [ ] **Step 3: Run the test**

```bash
cargo test --test eval_test
```

Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add examples/bool_demo.lc tests/eval_test.rs
git commit -m "test: end-to-end boolean reduction via example file"
```

---

### Task 16: End-to-end test — Y combinator + factorial of 3

**Files:**
- Create: `examples/fact.lc`
- Modify: `tests/eval_test.rs`

- [ ] **Step 1: Create the factorial example**

Write `examples/fact.lc` (note: order respects §2.4 — pairs before pred):

```
def true  = \t. \f. t
def false = \t. \f. f

def zero   = \f. \x. x
def succ   = \n. \f. \x. f (n f x)
def one    = succ zero
def two    = succ one
def three  = succ two
def mul    = \m. \n. \f. m (n f)
def isZero = \n. n (\u. false) true

def pair  = \a. \b. \s. s a b
def fst   = \p. p (\a. \b. a)
def snd   = \p. p (\a. \b. b)
def shift = \p. pair (snd p) (succ (snd p))
def pred  = \n. fst (n shift (pair zero zero))

def Y    = \f. (\x. f (x x)) (\x. f (x x))
def fact = Y (\rec. \n. (isZero n) one (mul n (rec (pred n))))

fact three
```

- [ ] **Step 2: Add a test that asserts the result β-equals `six`**

Comparing β-equivalent terms by string is fragile (different reduction orders → different normal forms might be produced; Church numerals always reduce to a canonical form `\f. \x. f^n x` though). For factorial-of-three the canonical Church numeral 6 is `\f. \x. f (f (f (f (f (f x)))))`.

Add to `tests/eval_test.rs`:

```rust
#[test]
fn fact_three_is_church_six() {
    let result = run("examples/fact.lc");
    let expected = "\\f. \\x. f (f (f (f (f (f x)))))";
    assert_eq!(result, expected);
}
```

- [ ] **Step 3: Run the test**

```bash
cargo test --test eval_test fact_three_is_church_six -- --nocapture
```

Expected: passes.

If it fails because the reduction step limit is too low, raise it in the test runner — but `fact 3` should comfortably finish under 100,000 steps with our reducer.

If it fails because the produced term is structurally `six` but not the exact canonical form (e.g., `f` and `x` are renamed to fresh names due to capture avoidance), update the expected string accordingly. Use `cargo test fact_three -- --nocapture` and inspect the actual output.

- [ ] **Step 4: Commit**

```bash
git add examples/fact.lc tests/eval_test.rs
git commit -m "test: factorial via Y combinator end-to-end"
```

---

**End of M2.** Real evaluator, capture-avoiding, demonstrably correct on the Y combinator. Time to make it usable.

---

## Milestone 3 — REPL + stdlib + Church-shape detection

End state: `cargo run` opens a REPL with `prelude.lc` pre-loaded; `fact (succ (succ (succ zero)))` displays as `6`.

### Task 17: Write `lib/prelude.lc`

**Files:**
- Create: `lib/prelude.lc`

- [ ] **Step 1: Write the prelude**

Create `lib/prelude.lc` with the contents below. Order respects the §2.4 substitution rule.

```
-- Tier 1: combinators
def id      = \x. x
def const   = \x. \y. x
def flip    = \f. \x. \y. f y x
def compose = \f. \g. \x. f (g x)

-- Tier 2: booleans
def true  = \t. \f. t
def false = \t. \f. f
def if    = \b. \t. \e. b t e
def not   = \p. p false true
def and   = \p. \q. p q p
def or    = \p. \q. p p q

-- Tier 3a: numerals (basic)
def zero   = \f. \x. x
def succ   = \n. \f. \x. f (n f x)
def one    = succ zero
def two    = succ one
def three  = succ two
def four   = succ three
def five   = succ four
def add    = \m. \n. \f. \x. m f (n f x)
def mul    = \m. \n. \f. m (n f)
def pow    = \m. \n. n m
def isZero = \n. n (\u. false) true

-- Tier 4: pairs
def pair  = \a. \b. \s. s a b
def fst   = \p. p (\a. \b. a)
def snd   = \p. p (\a. \b. b)
def shift = \p. pair (snd p) (succ (snd p))

-- Tier 3b: numerals using pairs
def pred = \n. fst (n shift (pair zero zero))
def sub  = \m. \n. n pred m

-- Tier 5: recursion
def Y    = \f. (\x. f (x x)) (\x. f (x x))
def fact = Y (\rec. \n. (isZero n) one (mul n (rec (pred n))))

-- Tier 6: lists
def nil    = \c. \n. n
def cons   = \h. \t. \c. \n. c h (t c n)
def isNil  = \l. l (\u. \v. false) true
def foldr  = \c. \n. \l. l c n
def map    = \f. \l. \c. \n. l (\h. c (f h)) n
def filter = \p. \l. \c. \n. l (\h. \r. (p h) (c h r) r) n
def append = \xs. \ys. \c. \n. xs c (ys c n)
def length = \l. l (\u. \r. succ r) zero
```

- [ ] **Step 2: Verify it parses**

Add a test to `tests/parser_test.rs`:

```rust
#[test]
fn prelude_parses() {
    let src = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let p = parse_program(&src).expect("prelude must parse");
    assert!(!p.defs.is_empty());
    assert!(p.main.is_none(), "prelude is a library; no main expression");
}
```

```bash
cargo test --test parser_test prelude_parses
```

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add lib/prelude.lc tests/parser_test.rs
git commit -m "feat: add lib/prelude.lc — full stdlib written in pure λ"
```

---

### Task 18: Load prelude when running a file; stdlib end-to-end test

**Files:**
- Modify: `src/main.rs`
- Create: `tests/stdlib_test.rs`

- [ ] **Step 1: Update `main.rs` to prepend prelude defs to user file**

Replace `src/main.rs`:

```rust
use std::env;
use std::process;

use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

const DEFAULT_STEP_LIMIT: usize = 1_000_000;
const PRELUDE_PATH: &str = "lib/prelude.lc";

fn load_prelude() -> Program {
    let src = std::fs::read_to_string(PRELUDE_PATH)
        .expect("could not read lib/prelude.lc; run from project root");
    parse_program(&src).expect("prelude failed to parse")
}

fn merge(prelude: Program, user: Program) -> Program {
    // Prelude defs go first (they have no forward references into user code),
    // then user defs (which may reference prelude), then user main.
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    Program { defs, main: user.main }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: lc <file.lc>");
        process::exit(2);
    }
    let path = &args[1];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { eprintln!("error reading {}: {}", path, e); process::exit(1); }
    };
    let user = match parse_program(&src) {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    };
    let program = merge(load_prelude(), user);
    if program.main.is_none() {
        for d in &program.defs {
            println!("def {} = {}", d.name, print(&d.body));
        }
        return;
    }
    let inlined = match inline_defs(&program) {
        Ok(e) => e,
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    };
    match normalize(inlined, DEFAULT_STEP_LIMIT) {
        Ok(nf) => println!("{}", print(&nf)),
        Err(e) => { eprintln!("{}", e); process::exit(1); }
    }
}
```

- [ ] **Step 2: Create stdlib integration test**

Write `tests/stdlib_test.rs`:

```rust
use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

fn run_with_prelude(user_src: &str) -> String {
    let prelude_src = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let prelude = parse_program(&prelude_src).unwrap();
    let user = parse_program(user_src).unwrap();
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    let program = Program { defs, main: user.main };
    let inlined = inline_defs(&program).unwrap();
    let nf = normalize(inlined, 1_000_000).unwrap();
    print(&nf)
}

#[test]
fn add_one_two_is_three() {
    let r = run_with_prelude("add one two");
    // Church 3 in canonical form
    assert_eq!(r, "\\f. \\x. f (f (f x))");
}

#[test]
fn fact_three_via_prelude() {
    let r = run_with_prelude("fact three");
    // Church 6
    assert_eq!(r, "\\f. \\x. f (f (f (f (f (f x)))))");
}

#[test]
fn list_length_three() {
    let r = run_with_prelude("length (cons one (cons two (cons three nil)))");
    // length [1,2,3] = three
    assert_eq!(r, "\\f. \\x. f (f (f x))");
}
```

- [ ] **Step 3: Run integration tests**

```bash
cargo test --test stdlib_test -- --test-threads=1
```

Expected: 3 tests pass. (Single-threaded because they all read `lib/prelude.lc` from disk; not strictly required, just polite.)

- [ ] **Step 4: Commit**

```bash
git add src/main.rs tests/stdlib_test.rs
git commit -m "feat: auto-load prelude when running .lc files; stdlib integration tests"
```

---

### Task 19: Church-shape detection in pretty-printer

The pretty-printer should recognize Church numerals, `true`, `false`, and pairs and print them in human-readable form. Detection happens at print time and is purely cosmetic — the underlying terms are unchanged.

**Files:**
- Modify: `src/pretty.rs`

- [ ] **Step 1: Add tests for shape detection**

Append to `src/pretty.rs` `tests` module:

```rust
    #[test]
    fn print_church_zero() {
        let zero = Expr::abs("f", Expr::abs("x", Expr::var("x")));
        assert_eq!(print(&zero), "0");
    }

    #[test]
    fn print_church_three() {
        // \f. \x. f (f (f x))
        let three = Expr::abs(
            "f",
            Expr::abs(
                "x",
                Expr::app(
                    Expr::var("f"),
                    Expr::app(
                        Expr::var("f"),
                        Expr::app(Expr::var("f"), Expr::var("x")),
                    ),
                ),
            ),
        );
        assert_eq!(print(&three), "3");
    }

    #[test]
    fn print_church_true() {
        // \t. \f. t
        let t = Expr::abs("t", Expr::abs("f", Expr::var("t")));
        assert_eq!(print(&t), "true");
    }

    #[test]
    fn print_church_false() {
        // \t. \f. f
        let f = Expr::abs("t", Expr::abs("f", Expr::var("f")));
        assert_eq!(print(&f), "false");
    }

    #[test]
    fn print_non_numeral_lambda_falls_through() {
        // \x. y  is not a Church numeral; print as raw lambda
        let e = Expr::abs("x", Expr::var("y"));
        assert_eq!(print(&e), "\\x. y");
    }
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cargo test --lib pretty
```

Expected: 5 new tests fail (no shape detection yet).

- [ ] **Step 3: Implement shape detection in `print`**

Modify `src/pretty.rs`:

```rust
use crate::ast::Expr;

pub fn print(e: &Expr) -> String {
    if let Some(n) = as_church_numeral(e) {
        return n.to_string();
    }
    if let Some(b) = as_church_boolean(e) {
        return if b { "true".into() } else { "false".into() };
    }
    print_expr(e)
}

/// Detect a Church numeral: `\f. \x. f (f (... (f x) ...))`. Returns the
/// number of `f`-applications.
fn as_church_numeral(e: &Expr) -> Option<u64> {
    if let Expr::Abs(f_name, body1) = e {
        if let Expr::Abs(x_name, body2) = body1.as_ref() {
            return count_applications(body2, f_name, x_name);
        }
    }
    None
}

fn count_applications(e: &Expr, f: &str, x: &str) -> Option<u64> {
    match e {
        Expr::Var(n) if n == x => Some(0),
        Expr::App(head, arg) => {
            if let Expr::Var(n) = head.as_ref() {
                if n == f {
                    return count_applications(arg, f, x).map(|k| k + 1);
                }
            }
            None
        }
        _ => None,
    }
}

/// Detect a Church boolean: `\t. \f. t` (true) or `\t. \f. f` (false).
fn as_church_boolean(e: &Expr) -> Option<bool> {
    if let Expr::Abs(t, inner) = e {
        if let Expr::Abs(f, body) = inner.as_ref() {
            if let Expr::Var(name) = body.as_ref() {
                if name == t { return Some(true); }
                if name == f { return Some(false); }
            }
        }
    }
    None
}

fn print_expr(e: &Expr) -> String {
    match e {
        Expr::Var(name) => name.clone(),
        Expr::Abs(param, body) => format!("\\{}. {}", param, print_expr(body)),
        Expr::App(f, x) => {
            let f_str = match **f {
                Expr::Abs(_, _) => format!("({})", print_expr(f)),
                _ => print_expr(f),
            };
            let x_str = match **x {
                Expr::App(_, _) | Expr::Abs(_, _) => format!("({})", print_expr(x)),
                _ => print_expr(x),
            };
            format!("{} {}", f_str, x_str)
        }
    }
}
```

- [ ] **Step 4: Run all pretty tests**

```bash
cargo test --lib pretty
```

Expected: all tests (originals + 5 new) pass.

- [ ] **Step 5: But wait — earlier eval tests asserted on raw lambda forms!**

The earlier integration tests like `add_one_two_is_three` and `fact_three_via_prelude` asserted strings like `"\\f. \\x. f (f (f x))"`. With shape detection, those will now print as `"3"` and `"6"` respectively. Update them:

In `tests/stdlib_test.rs`:

```rust
#[test]
fn add_one_two_is_three() {
    let r = run_with_prelude("add one two");
    assert_eq!(r, "3");
}

#[test]
fn fact_three_via_prelude() {
    let r = run_with_prelude("fact three");
    assert_eq!(r, "6");
}

#[test]
fn list_length_three() {
    let r = run_with_prelude("length (cons one (cons two (cons three nil)))");
    assert_eq!(r, "3");
}
```

In `tests/eval_test.rs`, update `fact_three_is_church_six`:

```rust
#[test]
fn fact_three_is_church_six() {
    let result = run("examples/fact.lc");
    assert_eq!(result, "6");
}
```

In `src/eval.rs` `tests`, update `not_true_reduces_to_false_form` (it was in `tests/eval_test.rs` actually) and similar — or leave them asserting on raw forms by calling `print_expr` directly. Cleanest fix: update integration tests as above.

- [ ] **Step 6: Run all tests**

```bash
cargo test
```

Expected: everything green.

- [ ] **Step 7: Commit**

```bash
git add src/pretty.rs tests/eval_test.rs tests/stdlib_test.rs
git commit -m "feat(pretty): detect Church numerals and booleans; update tests"
```

---

### Task 20: REPL — basic loop with `rustyline`

**Files:**
- Modify: `src/repl.rs`, `src/main.rs`

- [ ] **Step 1: Implement the REPL**

Replace `src/repl.rs`:

```rust
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::ast::{Def, Program};
use crate::eval::{inline_defs, normalize};
use crate::parser::parse_program;
use crate::pretty::print;

const STEP_LIMIT: usize = 1_000_000;

pub fn run(initial_defs: Vec<Def>) {
    let mut env: Vec<Def> = initial_defs;
    let mut rl = DefaultEditor::new().expect("readline");
    println!("lc — pure untyped λ-calculus REPL");
    println!("type :help for commands, :quit or Ctrl-D to exit");

    loop {
        let readline = rl.readline("λ> ");
        match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                if let Some(rest) = trimmed.strip_prefix(':') {
                    handle_command(rest, &env);
                    if rest.trim() == "quit" { break; }
                    continue;
                }

                evaluate(trimmed, &mut env);
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => break,
            Err(e) => {
                eprintln!("readline error: {}", e);
                break;
            }
        }
    }
}

fn handle_command(cmd: &str, env: &[Def]) {
    let cmd = cmd.trim();
    match cmd {
        "quit" | "q" => {}
        "help" | "h" => {
            println!(":help                 show this");
            println!(":env                  list current definitions");
            println!(":quit                 exit");
            println!("def <name> = <expr>   add a definition");
            println!("<expr>                evaluate an expression");
        }
        "env" => {
            for d in env {
                println!("def {} = {}", d.name, print(&d.body));
            }
        }
        other => eprintln!("unknown command: :{}", other),
    }
}

fn evaluate(line: &str, env: &mut Vec<Def>) {
    let parsed = match parse_program(line) {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); return; }
    };
    // Add new defs to env.
    for d in parsed.defs { env.push(d); }
    // Evaluate main if present.
    if let Some(main) = parsed.main {
        let program = Program { defs: env.clone(), main: Some(main) };
        match inline_defs(&program) {
            Ok(e) => match normalize(e, STEP_LIMIT) {
                Ok(nf) => println!("{}", print(&nf)),
                Err(err) => eprintln!("{}", err),
            },
            Err(err) => eprintln!("{}", err),
        }
    }
}
```

- [ ] **Step 2: Wire `main.rs` to fall into the REPL when called without args**

Replace the `if args.len() < 2` block in `src/main.rs`:

```rust
    if args.len() < 2 {
        let prelude = load_prelude();
        lc::repl::run(prelude.defs);
        return;
    }
```

- [ ] **Step 3: Try the REPL manually**

```bash
cargo run
```

Then type:

```
λ> add one two
3
λ> fact three
6
λ> :env
... lots of defs ...
λ> :quit
```

(Verify each line works; this is a smoke test.)

- [ ] **Step 4: Commit**

```bash
git add src/repl.rs src/main.rs
git commit -m "feat(repl): rustyline-based interactive loop with :env, :help, :quit"
```

---

### Task 21: Final cleanup and README

**Files:**
- Create: `README.md`
- Modify: `Cargo.toml` (description, license)

- [ ] **Step 1: Add a minimal README**

Write `README.md`:

```markdown
# lc — minimal lambda-calculus language

Pure untyped λ-calculus, written in Rust. Standard library written in the language itself.

## Usage

```bash
cargo run                     # REPL with prelude.lc preloaded
cargo run -- examples/fact.lc # run a file
cargo test                    # run all tests
```

## Status

Phase A complete: working call-by-name normal-order tree-walking interpreter.
Phase B (call-by-need) and Phase C (optimizations, abstract machines) planned.

See [docs/superpowers/specs](docs/superpowers/specs) for the full design.
```

- [ ] **Step 2: Update `Cargo.toml` description**

In `Cargo.toml`, under `[package]`, ensure:

```toml
description = "A minimal pure-untyped-λ interpreter for learning"
license = "MIT OR Apache-2.0"
```

- [ ] **Step 3: Run the full test suite one last time**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add README.md Cargo.toml
git commit -m "docs: README and package metadata"
```

---

**End of Phase A.** You have a complete, correct, slow λ-calculus interpreter:

- Pure untyped λ-calculus AST (`Var | Abs | App`).
- `chumsky`-based parser with comments, defs, programs.
- Capture-avoiding substitution; leftmost-outermost reduction; step limit.
- `def` inlining with cross-def references.
- Auto-loaded `lib/prelude.lc` with combinators, booleans, numerals, pairs, Y, fact, lists.
- REPL with line editing, history, env inspection.
- Pretty-printer that recognizes Church numerals and booleans.

Next: Phase B (M4 De Bruijn indices, M5 call-by-need lazy evaluation). That plan should be written when Phase A is done — the implementation experience here will inform decisions in B.
