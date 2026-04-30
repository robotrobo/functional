# Hindley–Milner Type Inference (Advisory) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Hindley–Milner type inference plus a `fix` primitive to the existing untyped λ-calculus interpreter, in **advisory mode**: types are inferred and printed, type errors are reported, but evaluation always runs.

**Architecture:** Two new modules — `src/types.rs` (Type, Scheme, Subst) and `src/infer.rs` (Algorithm W: unification with occurs check, generalize, instantiate, infer). One new `Expr::Fix` variant added to the AST and threaded through every existing pass (parser, evaluator, simplifier, strictness analysis, de Bruijn translation, pretty-printer). The REPL and file runner call `infer_program` before evaluation, print `: <type>` lines for each def and main, and continue to evaluate even if inference fails. No prelude changes; existing behavior is preserved.

**Tech stack:** Existing — Rust 2021, `chumsky` 0.9, `rustyline` 14, `thiserror`. No new dependencies.

**Known design caveats (intentional, deferred to later strict-mode plan):**
- Inner `let x = e1 in e2` is parser-desugared to `(\x. e2) e1`, so `x` does **not** get let-generalization. Top-level `def`s do generalize. This is documented; revisit when promoting from advisory to strict.
- The HM inference of Church-encoded `if`, `pair`, etc. yields ugly but valid types. We accept that; we do not introduce primitive `Bool`/`Nat` in this plan.
- Type errors are location-free (no spans). The existing AST has no spans; adding them is out of scope.

---

## File Structure

```
src/
  ast.rs           -- ADD: Expr::Fix(Box<Expr>) variant
  parser.rs        -- ADD: `fix` keyword → Expr::Fix
  pretty.rs        -- ADD: print arm for Expr::Fix
  eval.rs          -- ADD: subst/free_vars/reduce_step/alpha_eq/inline_defs arms for Fix
  simplify.rs      -- ADD: arm for Fix (default: do not simplify under Fix)
  strict.rs        -- ADD: arm for Fix
  debruijn.rs      -- ADD: to_db/to_named arms for Fix
  cbn.rs           -- ADD: WHNF rule for Fix (unfold once per force)
  types.rs         -- NEW: Type, Scheme, TVarId, Subst, type pretty-printing
  infer.rs         -- NEW: Algorithm W, unify, generalize, instantiate, infer_program
  type_error.rs    -- NEW: TypeError enum (Display via thiserror)
  lib.rs           -- ADD: pub mod types; pub mod infer; pub mod type_error;
  repl.rs          -- MODIFY: call infer_program, print types or type errors, then evaluate
  main.rs          -- MODIFY: file mode prints types alongside evaluation result

tests/
  types_test.rs    -- NEW: unit tests for unification + tiny end-to-end inference
  infer_prelude_test.rs -- NEW: load prelude.lc, assert which defs typecheck and which don't (snapshot)
```

Each file has one responsibility. `infer.rs` will be the largest new file (~250 lines). `types.rs` stays under 200.

**Spec reference:** designed conversationally; no separate spec doc.

---

## Milestone 1 — Type AST and Unification

End state: `Type`, `Scheme`, `Subst` exist; unification with occurs check is correct and unit-tested. Nothing is hooked into the rest of the codebase yet.

### Task 1: Add `Type` and `Scheme` to `src/types.rs`

**Files:**
- Create: `src/types.rs`
- Modify: `src/lib.rs:1-11` (add `pub mod types;`)

- [ ] **Step 1: Write the failing test**

Create `src/types.rs` with this content:

```rust
use std::collections::HashMap;

pub type TVarId = u32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Var(TVarId),
    Arrow(Box<Type>, Box<Type>),
}

impl Type {
    pub fn var(id: TVarId) -> Self { Type::Var(id) }
    pub fn arrow(a: Type, b: Type) -> Self { Type::Arrow(Box::new(a), Box::new(b)) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub vars: Vec<TVarId>,
    pub ty: Type,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_arrow_type() {
        let t = Type::arrow(Type::var(0), Type::var(0));
        assert_eq!(t, Type::Arrow(Box::new(Type::Var(0)), Box::new(Type::Var(0))));
    }

    #[test]
    fn build_scheme() {
        let s = Scheme { vars: vec![0], ty: Type::arrow(Type::var(0), Type::var(0)) };
        assert_eq!(s.vars, vec![0]);
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod ast;
pub mod parser;
pub mod pretty;
pub mod eval;
pub mod error;
pub mod repl;
pub mod debruijn;
pub mod cbn;
pub mod simplify;
pub mod strict;
pub mod types;
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib types::tests`
Expected: 2 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/types.rs src/lib.rs
git commit -m "types: Type, Scheme, TVarId scaffolding"
```

---

### Task 2: Substitutions and `apply`

**Files:**
- Modify: `src/types.rs` (append Subst + apply impls)

- [ ] **Step 1: Write the failing test**

Append to `src/types.rs`:

```rust
#[derive(Clone, Debug, Default)]
pub struct Subst(pub HashMap<TVarId, Type>);

impl Subst {
    pub fn empty() -> Self { Subst(HashMap::new()) }

    pub fn singleton(id: TVarId, ty: Type) -> Self {
        let mut m = HashMap::new();
        m.insert(id, ty);
        Subst(m)
    }

    /// Apply this substitution to a type, recursively. Variables not bound
    /// by the substitution are left alone; bound ones are replaced and the
    /// result is itself substituted (no need for a fixed point loop because
    /// `compose` keeps the map idempotent — see Task 2's compose impl).
    pub fn apply(&self, t: &Type) -> Type {
        match t {
            Type::Var(id) => match self.0.get(id) {
                Some(replacement) => replacement.clone(),
                None => t.clone(),
            },
            Type::Arrow(a, b) => Type::arrow(self.apply(a), self.apply(b)),
        }
    }

    pub fn apply_scheme(&self, s: &Scheme) -> Scheme {
        // Don't substitute bound vars (the ∀-quantified ones).
        let mut filtered = self.clone();
        for v in &s.vars {
            filtered.0.remove(v);
        }
        Scheme { vars: s.vars.clone(), ty: filtered.apply(&s.ty) }
    }

    /// `self ∘ other` — apply other first, then self. Maintains idempotency:
    /// for every (v, t) we keep, t already has `self` applied.
    pub fn compose(&self, other: &Subst) -> Subst {
        let mut out: HashMap<TVarId, Type> = other
            .0
            .iter()
            .map(|(k, v)| (*k, self.apply(v)))
            .collect();
        for (k, v) in &self.0 {
            out.entry(*k).or_insert_with(|| v.clone());
        }
        Subst(out)
    }
}

#[cfg(test)]
mod subst_tests {
    use super::*;

    #[test]
    fn apply_to_var_replaces() {
        let s = Subst::singleton(0, Type::arrow(Type::var(1), Type::var(1)));
        let result = s.apply(&Type::var(0));
        assert_eq!(result, Type::arrow(Type::var(1), Type::var(1)));
    }

    #[test]
    fn apply_to_unbound_var_is_noop() {
        let s = Subst::singleton(0, Type::var(99));
        assert_eq!(s.apply(&Type::var(7)), Type::var(7));
    }

    #[test]
    fn apply_recurses_under_arrow() {
        let s = Subst::singleton(0, Type::var(1));
        let t = Type::arrow(Type::var(0), Type::arrow(Type::var(0), Type::var(2)));
        let expected = Type::arrow(Type::var(1), Type::arrow(Type::var(1), Type::var(2)));
        assert_eq!(s.apply(&t), expected);
    }

    #[test]
    fn compose_applies_left_after_right() {
        // s1 = {0 → 1}, s2 = {1 → 2}, (s2 ∘ s1)(0) should be 2 (apply s1, then s2).
        let s1 = Subst::singleton(0, Type::var(1));
        let s2 = Subst::singleton(1, Type::var(2));
        let composed = s2.compose(&s1);
        assert_eq!(composed.apply(&Type::var(0)), Type::var(2));
    }

    #[test]
    fn apply_scheme_skips_bound_vars() {
        // Scheme: ∀a. a → a   (var 0 = a)
        // Subst: {0 → Int-ish (use var 99)} — should NOT touch the bound a.
        let scheme = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::var(0), Type::var(0)),
        };
        let s = Subst::singleton(0, Type::var(99));
        assert_eq!(s.apply_scheme(&scheme), scheme);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib types::subst_tests`
Expected: 5 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "types: Subst with apply, apply_scheme, compose"
```

---

### Task 3: Free type variables (`ftv`)

**Files:**
- Modify: `src/types.rs` (append `ftv` for Type and Scheme)

- [ ] **Step 1: Write the failing test**

Append to `src/types.rs`:

```rust
use std::collections::HashSet;

impl Type {
    pub fn ftv(&self) -> HashSet<TVarId> {
        let mut out = HashSet::new();
        self.collect_ftv(&mut out);
        out
    }

    fn collect_ftv(&self, out: &mut HashSet<TVarId>) {
        match self {
            Type::Var(id) => { out.insert(*id); }
            Type::Arrow(a, b) => { a.collect_ftv(out); b.collect_ftv(out); }
        }
    }
}

impl Scheme {
    pub fn ftv(&self) -> HashSet<TVarId> {
        let mut tv = self.ty.ftv();
        for v in &self.vars { tv.remove(v); }
        tv
    }
}

#[cfg(test)]
mod ftv_tests {
    use super::*;

    #[test]
    fn ftv_of_var() {
        assert_eq!(Type::var(3).ftv(), [3].into_iter().collect());
    }

    #[test]
    fn ftv_of_arrow() {
        let t = Type::arrow(Type::var(1), Type::arrow(Type::var(2), Type::var(1)));
        assert_eq!(t.ftv(), [1, 2].into_iter().collect());
    }

    #[test]
    fn scheme_ftv_excludes_bound() {
        // ∀a. a → b   (a = 0, b = 1)
        let s = Scheme { vars: vec![0], ty: Type::arrow(Type::var(0), Type::var(1)) };
        assert_eq!(s.ftv(), [1].into_iter().collect());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib types::ftv_tests`
Expected: 3 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "types: ftv for Type and Scheme"
```

---

### Task 4: Unification with occurs check

**Files:**
- Create: `src/type_error.rs`
- Modify: `src/lib.rs` (add `pub mod type_error;`)
- Modify: `src/types.rs` (append `unify` function)

- [ ] **Step 1: Create `src/type_error.rs`**

```rust
use thiserror::Error;
use crate::types::Type;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("cannot unify {0:?} with {1:?}")]
    Mismatch(Type, Type),

    #[error("occurs check: cannot construct infinite type t{0} = {1:?}")]
    OccursCheck(u32, Type),

    #[error("unbound variable in type checking: {0}")]
    UnboundVar(String),
}
```

Add to `src/lib.rs`:

```rust
pub mod type_error;
```

- [ ] **Step 2: Write the failing unify test**

Append to `src/types.rs`:

```rust
use crate::type_error::TypeError;

/// Unify two types, returning the most general unifier. Implements the
/// classical Robinson algorithm with occurs check. The returned substitution,
/// when applied to either input, yields equal types.
pub fn unify(a: &Type, b: &Type) -> Result<Subst, TypeError> {
    match (a, b) {
        (Type::Var(x), Type::Var(y)) if x == y => Ok(Subst::empty()),
        (Type::Var(x), t) | (t, Type::Var(x)) => bind(*x, t),
        (Type::Arrow(a1, b1), Type::Arrow(a2, b2)) => {
            let s1 = unify(a1, a2)?;
            let s2 = unify(&s1.apply(b1), &s1.apply(b2))?;
            Ok(s2.compose(&s1))
        }
    }
}

fn bind(x: TVarId, t: &Type) -> Result<Subst, TypeError> {
    if let Type::Var(y) = t { if *y == x { return Ok(Subst::empty()); } }
    if t.ftv().contains(&x) {
        return Err(TypeError::OccursCheck(x, t.clone()));
    }
    Ok(Subst::singleton(x, t.clone()))
}

#[cfg(test)]
mod unify_tests {
    use super::*;

    #[test]
    fn unify_var_with_var() {
        let s = unify(&Type::var(0), &Type::var(1)).unwrap();
        // Either {0 → 1} or {1 → 0} is acceptable; they're symmetric. Just
        // check the substitution makes both sides equal.
        assert_eq!(s.apply(&Type::var(0)), s.apply(&Type::var(1)));
    }

    #[test]
    fn unify_same_var_is_empty() {
        let s = unify(&Type::var(7), &Type::var(7)).unwrap();
        assert!(s.0.is_empty());
    }

    #[test]
    fn unify_var_with_arrow() {
        // 0 ~ (1 → 2)
        let arr = Type::arrow(Type::var(1), Type::var(2));
        let s = unify(&Type::var(0), &arr).unwrap();
        assert_eq!(s.apply(&Type::var(0)), arr);
    }

    #[test]
    fn unify_occurs_check_fails() {
        // 0 ~ (0 → 1) should fail
        let arr = Type::arrow(Type::var(0), Type::var(1));
        let err = unify(&Type::var(0), &arr).unwrap_err();
        assert!(matches!(err, TypeError::OccursCheck(0, _)));
    }

    #[test]
    fn unify_arrows_recursive() {
        // (0 → 1) ~ (Int-ish → Bool-ish), using vars: (2 → 3)
        let lhs = Type::arrow(Type::var(0), Type::var(1));
        let rhs = Type::arrow(Type::var(2), Type::var(3));
        let s = unify(&lhs, &rhs).unwrap();
        assert_eq!(s.apply(&lhs), s.apply(&rhs));
    }

    #[test]
    fn unify_chained() {
        // (0 → 0) ~ (1 → 2) — must force 1 = 2
        let lhs = Type::arrow(Type::var(0), Type::var(0));
        let rhs = Type::arrow(Type::var(1), Type::var(2));
        let s = unify(&lhs, &rhs).unwrap();
        assert_eq!(s.apply(&Type::var(1)), s.apply(&Type::var(2)));
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib types::unify_tests`
Expected: 6 passing tests.

- [ ] **Step 4: Commit**

```bash
git add src/types.rs src/type_error.rs src/lib.rs
git commit -m "types: unify with occurs check"
```

---

## Milestone 2 — Algorithm W

End state: `infer_expr` correctly types `Var`, `Abs`, `App` (no `Fix` yet). Top-level `def` chains generalize correctly via `infer_program`.

### Task 5: TypeEnv and fresh-variable supply

**Files:**
- Create: `src/infer.rs`
- Modify: `src/lib.rs` (add `pub mod infer;`)

- [ ] **Step 1: Create `src/infer.rs`**

```rust
use std::collections::HashMap;

use crate::types::{Scheme, Subst, TVarId, Type};

#[derive(Clone, Default, Debug)]
pub struct TypeEnv(pub HashMap<String, Scheme>);

impl TypeEnv {
    pub fn empty() -> Self { TypeEnv(HashMap::new()) }

    pub fn insert(&self, name: impl Into<String>, scheme: Scheme) -> Self {
        let mut next = self.0.clone();
        next.insert(name.into(), scheme);
        TypeEnv(next)
    }

    pub fn apply_subst(&self, s: &Subst) -> TypeEnv {
        TypeEnv(self.0.iter().map(|(k, v)| (k.clone(), s.apply_scheme(v))).collect())
    }

    pub fn ftv(&self) -> std::collections::HashSet<TVarId> {
        let mut out = std::collections::HashSet::new();
        for s in self.0.values() { out.extend(s.ftv()); }
        out
    }
}

pub struct Fresh { next: TVarId }

impl Fresh {
    pub fn new() -> Self { Fresh { next: 0 } }
    pub fn tvar(&mut self) -> Type {
        let id = self.next;
        self.next += 1;
        Type::Var(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_yields_distinct_vars() {
        let mut f = Fresh::new();
        let a = f.tvar();
        let b = f.tvar();
        assert_ne!(a, b);
    }

    #[test]
    fn env_insert_does_not_mutate_original() {
        let e1 = TypeEnv::empty();
        let _e2 = e1.insert("x", Scheme { vars: vec![], ty: Type::var(0) });
        assert!(e1.0.is_empty());
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod infer;
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib infer::tests`
Expected: 2 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/infer.rs src/lib.rs
git commit -m "infer: TypeEnv and Fresh tvar supply"
```

---

### Task 6: Generalize and instantiate

**Files:**
- Modify: `src/infer.rs` (append generalize, instantiate)

- [ ] **Step 1: Write the failing test**

Append to `src/infer.rs`:

```rust
/// Quantify over every type variable that's free in `t` but NOT free in `env`.
/// This is the only place ∀ binders are introduced.
pub fn generalize(env: &TypeEnv, t: Type) -> Scheme {
    let env_ftv = env.ftv();
    let mut quantified: Vec<TVarId> = t.ftv().into_iter().filter(|v| !env_ftv.contains(v)).collect();
    quantified.sort();
    Scheme { vars: quantified, ty: t }
}

/// Replace every quantified variable in the scheme with a fresh tvar.
/// This is how a let-bound polymorphic identifier becomes a monotype at a
/// specific use site.
pub fn instantiate(scheme: &Scheme, fresh: &mut Fresh) -> Type {
    let mut subst = Subst::empty();
    for v in &scheme.vars {
        subst.0.insert(*v, fresh.tvar());
    }
    subst.apply(&scheme.ty)
}

#[cfg(test)]
mod gen_inst_tests {
    use super::*;

    #[test]
    fn generalize_quantifies_unbound_vars() {
        // env: empty; type: 0 → 0  ⇒  ∀0. 0 → 0
        let env = TypeEnv::empty();
        let t = Type::arrow(Type::var(0), Type::var(0));
        let s = generalize(&env, t);
        assert_eq!(s.vars, vec![0]);
    }

    #[test]
    fn generalize_skips_env_bound_vars() {
        // env has a scheme that mentions tvar 1 (free); type 1 → 0.
        // 1 is bound in env so only 0 should be quantified.
        let env_scheme = Scheme { vars: vec![], ty: Type::var(1) };
        let env = TypeEnv::empty().insert("x", env_scheme);
        let t = Type::arrow(Type::var(1), Type::var(0));
        let s = generalize(&env, t);
        assert_eq!(s.vars, vec![0]);
    }

    #[test]
    fn instantiate_renames_bound_vars_to_fresh() {
        // ∀a. a → a — instantiate twice; the two fresh tvars must be distinct.
        let scheme = Scheme { vars: vec![0], ty: Type::arrow(Type::var(0), Type::var(0)) };
        let mut fresh = Fresh::new();
        let t1 = instantiate(&scheme, &mut fresh);
        let t2 = instantiate(&scheme, &mut fresh);
        assert_ne!(t1, t2);
        // Each instantiation is itself an "α → α" shape (same on both sides).
        if let Type::Arrow(a, b) = &t1 { assert_eq!(a, b); } else { panic!(); }
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib infer::gen_inst_tests`
Expected: 3 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/infer.rs
git commit -m "infer: generalize and instantiate"
```

---

### Task 7: `infer_expr` for Var, Abs, App

**Files:**
- Modify: `src/infer.rs` (append `infer_expr`)

- [ ] **Step 1: Write the failing test**

Append to `src/infer.rs`:

```rust
use crate::ast::Expr;
use crate::type_error::TypeError;
use crate::types::unify;

/// Algorithm W. Returns (substitution, type). On error, returns TypeError.
/// The substitution must be applied to the result type (and to any caller
/// state) for the type to be "principal" at this point.
pub fn infer_expr(env: &TypeEnv, e: &Expr, fresh: &mut Fresh) -> Result<(Subst, Type), TypeError> {
    match e {
        Expr::Var(name) => {
            let scheme = env.0.get(name)
                .ok_or_else(|| TypeError::UnboundVar(name.clone()))?;
            Ok((Subst::empty(), instantiate(scheme, fresh)))
        }
        Expr::Abs(param, body) => {
            let alpha = fresh.tvar();
            let scheme = Scheme { vars: vec![], ty: alpha.clone() };
            let env2 = env.insert(param.clone(), scheme);
            let (s, t_body) = infer_expr(&env2, body, fresh)?;
            let arrow = Type::arrow(s.apply(&alpha), t_body);
            Ok((s, arrow))
        }
        Expr::App(e1, e2) => {
            let (s1, t1) = infer_expr(env, e1, fresh)?;
            let env2 = env.apply_subst(&s1);
            let (s2, t2) = infer_expr(&env2, e2, fresh)?;
            let alpha = fresh.tvar();
            let s3 = unify(&s2.apply(&t1), &Type::arrow(t2, alpha.clone()))?;
            let composed = s3.compose(&s2).compose(&s1);
            Ok((composed, s3.apply(&alpha)))
        }
    }
}

#[cfg(test)]
mod infer_expr_tests {
    use super::*;
    use crate::ast::Expr;

    fn infer(e: &Expr) -> Result<Type, TypeError> {
        let mut fresh = Fresh::new();
        let (s, t) = infer_expr(&TypeEnv::empty(), e, &mut fresh)?;
        Ok(s.apply(&t))
    }

    #[test]
    fn identity_lambda_is_polymorphic() {
        // \x. x  ⇒  α → α  (for some α)
        let e = Expr::abs("x", Expr::var("x"));
        let t = infer(&e).unwrap();
        if let Type::Arrow(a, b) = &t { assert_eq!(a, b); } else { panic!("not an arrow"); }
    }

    #[test]
    fn const_lambda_two_distinct_vars() {
        // \x. \y. x  ⇒  α → β → α
        let e = Expr::abs("x", Expr::abs("y", Expr::var("x")));
        let t = infer(&e).unwrap();
        match t {
            Type::Arrow(a, rest) => match *rest {
                Type::Arrow(_b, c) => assert_eq!(*a, *c),
                _ => panic!("expected nested arrow"),
            },
            _ => panic!("expected arrow"),
        }
    }

    #[test]
    fn application_of_identity() {
        // (\x. x) (\y. y)  should typecheck and yield an α → α
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let t = infer(&e).unwrap();
        if let Type::Arrow(a, b) = &t { assert_eq!(a, b); } else { panic!(); }
    }

    #[test]
    fn unbound_variable_errors() {
        let e = Expr::var("nope");
        let err = infer(&e).unwrap_err();
        assert!(matches!(err, TypeError::UnboundVar(_)));
    }

    #[test]
    fn omega_self_application_fails_occurs_check() {
        // \x. x x  ⇒  occurs check
        let e = Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x")));
        let err = infer(&e).unwrap_err();
        assert!(matches!(err, TypeError::OccursCheck(..)));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib infer::infer_expr_tests`
Expected: 5 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/infer.rs
git commit -m "infer: Algorithm W for Var/Abs/App"
```

---

### Task 8: `infer_program` — top-level def generalization

**Files:**
- Modify: `src/infer.rs` (append `infer_program`, `ProgramTypes`)

- [ ] **Step 1: Write the failing test**

Append to `src/infer.rs`:

```rust
use crate::ast::Program;

/// One slot per def, plus optional `main`. Each def's scheme is the
/// generalized type at the point of its definition; `main_type` is the
/// monotype of the main expression in the final environment.
#[derive(Debug, Default)]
pub struct ProgramTypes {
    pub defs: Vec<(String, Result<Scheme, TypeError>)>,
    pub main_type: Option<Result<Type, TypeError>>,
}

/// Type-check a program in advisory mode: every def is checked independently.
/// A def that fails to typecheck is recorded as an Err but does NOT abort —
/// later defs see an environment without it (so they may also fail with
/// UnboundVar when they reference it). Main is checked last.
pub fn infer_program(p: &Program) -> ProgramTypes {
    let mut env = TypeEnv::empty();
    let mut fresh = Fresh::new();
    let mut out = ProgramTypes::default();

    for d in &p.defs {
        match infer_expr(&env, &d.body, &mut fresh) {
            Ok((s, t)) => {
                let env_after = env.apply_subst(&s);
                let scheme = generalize(&env_after, s.apply(&t));
                env = env_after.insert(d.name.clone(), scheme.clone());
                out.defs.push((d.name.clone(), Ok(scheme)));
            }
            Err(e) => {
                out.defs.push((d.name.clone(), Err(e)));
                // Continue without binding `name` — references in later
                // defs/main will get UnboundVar.
            }
        }
    }

    if let Some(main) = &p.main {
        let r = infer_expr(&env, main, &mut fresh).map(|(s, t)| s.apply(&t));
        out.main_type = Some(r);
    }

    out
}

#[cfg(test)]
mod program_tests {
    use super::*;
    use crate::ast::{Def, Expr, Program};

    #[test]
    fn def_uses_polymorphic_id_at_two_types() {
        // def id = \x. x
        // main = id (\y. y)            ← should typecheck
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: Some(Expr::app(Expr::var("id"), Expr::abs("y", Expr::var("y")))),
        };
        let types = infer_program(&p);
        assert!(types.defs[0].1.is_ok(), "id should typecheck");
        assert!(types.main_type.unwrap().is_ok(), "main should typecheck");
    }

    #[test]
    fn ill_typed_def_is_recorded_but_does_not_abort() {
        // def bad = \x. x x        ← occurs check
        // def good = \x. x
        let p = Program {
            defs: vec![
                Def {
                    name: "bad".into(),
                    body: Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x"))),
                },
                Def {
                    name: "good".into(),
                    body: Expr::abs("x", Expr::var("x")),
                },
            ],
            main: None,
        };
        let types = infer_program(&p);
        assert!(types.defs[0].1.is_err(), "bad should fail");
        assert!(types.defs[1].1.is_ok(), "good should still succeed");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib infer::program_tests`
Expected: 2 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/infer.rs
git commit -m "infer: program-level inference with let-generalization for defs"
```

---

## Milestone 3 — `fix` Primitive

End state: `fix e` parses, evaluates correctly (β-equivalent to `e (fix e)`), and typechecks at `∀a. (a → a) → a`. Existing tests still pass.

### Task 9: Add `Expr::Fix` variant and update all match sites

**Files:**
- Modify: `src/ast.rs` (add variant + builder)
- Modify: `src/eval.rs` (subst, free_vars, reduce_step, alpha_eq, lines containing `match` over Expr)
- Modify: `src/simplify.rs`
- Modify: `src/strict.rs`
- Modify: `src/debruijn.rs`
- Modify: `src/cbn.rs`
- Modify: `src/pretty.rs` (print_expr arm + parenthesization rule)

**This is the heaviest task. It compiles only after every match site adds a `Fix` arm.**

- [ ] **Step 1: Modify `src/ast.rs`**

Update `Expr` and add a builder:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Var(String),
    Abs(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Fix(Box<Expr>),
}

impl Expr {
    pub fn var(name: impl Into<String>) -> Self { Expr::Var(name.into()) }
    pub fn abs(param: impl Into<String>, body: Expr) -> Self { Expr::Abs(param.into(), Box::new(body)) }
    pub fn app(f: Expr, x: Expr) -> Self { Expr::App(Box::new(f), Box::new(x)) }
    pub fn fix(e: Expr) -> Self { Expr::Fix(Box::new(e)) }
}
```

- [ ] **Step 2: Update `src/eval.rs` match sites**

In `subst`, append a `Fix` arm at the bottom of the inner `match target`:

```rust
Expr::Fix(inner) => Expr::fix(subst(inner, x, value)),
```

In `free_vars`, append:

```rust
Expr::Fix(inner) => free_vars(inner),
```

In `reduce_step`, replace the `_ => None,` fallthrough with explicit handling for `Var` and `Fix`:

```rust
Expr::Var(_) => None,
Expr::Fix(inner) => {
    if let Expr::Abs(_, _) = &**inner {
        // fix (\x. body)  ↪  (\x. body) (fix (\x. body))
        Some(Expr::app((**inner).clone(), Expr::fix((**inner).clone())))
    } else {
        // Reduce inside until it's an Abs.
        reduce_step(inner).map(Expr::fix)
    }
}
```

Remove the `_ => None,` line; the match is now exhaustive.

In `alpha_eq_with`, add a new `(Expr::Fix(a), Expr::Fix(b))` arm before the catch-all:

```rust
(Expr::Fix(x), Expr::Fix(y)) => alpha_eq_with(x, y, env_a, env_b),
```

- [ ] **Step 3: Update `src/pretty.rs`**

In `print_expr`'s match, add an arm:

```rust
Expr::Fix(inner) => {
    let inner_str = match **inner {
        Expr::Abs(_, _) | Expr::App(_, _) | Expr::Fix(_) => format!("({})", print_expr(inner)),
        _ => print_expr(inner),
    };
    format!("fix {}", inner_str)
}
```

Also, in the `App`'s argument-parenthesization arm in `print_expr`, add `Expr::Fix(_)` so a `fix` argument gets parens too:

```rust
let x_str = match **x {
    Expr::App(_, _) | Expr::Abs(_, _) | Expr::Fix(_) => format!("({})", print_expr(x)),
    _ => print_expr(x),
};
```

And in the function-position arm, add `Expr::Fix(_)` if needed (only matters for clarity — `fix f x` should print as `fix f x`, not `(fix f) x`. Use `App(_,_)` style: do NOT parenthesize on the function side):

(Leave the function-position parenthesization unchanged — `fix f` followed by `x` should pretty-print as `fix f x`.)

- [ ] **Step 4: Update `src/debruijn.rs`**

Add a `Fix(Rc<DBExpr>)` variant to `DBExpr` and thread it through.

(a) In the `pub enum DBExpr` definition (around line 23), add a new variant after `StrictApp`:

```rust
/// Fixed-point primitive. `fix e` evaluates to `e (fix e)`. Unfolded by
/// `cbn::whnf` when focused. Carries no binder of its own.
Fix(Rc<DBExpr>),
```

(b) In the manual `PartialEq` impl (around line 39), add an arm before the `_ => false,` catch-all:

```rust
(DBExpr::Fix(a), DBExpr::Fix(b)) => a == b,
```

(c) In the `impl DBExpr` builders block (around line 56), add a builder:

```rust
pub fn fix(e: DBExpr) -> Self {
    DBExpr::Fix(Rc::new(e))
}
```

(d) In `to_db`'s inner `go` (around line 161), add a `Fix` arm at the bottom of the inner `match e`:

```rust
Expr::Fix(inner) => DBExpr::fix(go(inner, env)),
```

(e) In `to_named`'s `Step` enum (around line 194), add a new variant:

```rust
BuildFix,
```

(f) In `to_named`'s `Step::Process` arm's inner `match e` (around line 206), add an arm and a corresponding `BuildFix` handler. After the `DBExpr::App | DBExpr::StrictApp` arm, add:

```rust
DBExpr::Fix(inner) => {
    work.push(Step::BuildFix);
    work.push(Step::Process(inner));
}
```

And add a `Step::BuildFix => { let inner = done.pop().unwrap(); done.push(Expr::fix(inner)); }` arm in the outer `match step`. Locate the `Step::BuildApp` arm and add `Step::BuildFix` next to it with the analogous one-child pop-and-wrap logic.

(g) In `subst`/`shift` helpers above (the file has these at lines 82, 110, 135 according to the grep), add a `DBExpr::Fix(inner) => DBExpr::fix(<recurse>(inner, ...))` arm to each match. Read each helper's signature and apply the recursive call with the same arguments.

- [ ] **Step 5: Update `src/cbn.rs`**

`whnf` is the WHNF state machine in `src/cbn.rs` (starts ~line 173). It's a focus/env/stack loop: when focus is `Fix(inner)`, we unfold one step. Push the (fix inner) thunk as the App-arg, focus on the inner expression — the next iteration will see whatever `inner` is and proceed.

In the `match focus` block inside `whnf`'s outer `loop`, add an arm (place it after the `DBExpr::StrictApp` arm and before `DBExpr::Abs`):

```rust
DBExpr::Fix(inner) => {
    // fix e  ↪  e (fix e). Push Fix(inner) as the App argument; focus
    // becomes inner. Next iteration applies the inner term to the
    // self-reference. Charge one budget tick per unfold.
    budget.tick()?;
    let self_ref = DBExpr::Fix(inner.clone());
    stack.push(Frame::Arg(self_ref, env.clone()));
    focus = (*inner).clone();
}
```

`nf` (around line 304) does NOT need a Fix arm: it calls `whnf` to reduce to a Value, and `whnf` always reduces a Fix away (or exhausts budget). The Closure / Neutral that whnf returns will not contain a top-level Fix.

If a `DBExpr::Fix` ever leaks into the test scaffolding at lines 382–520 (which inspect Pending thunks, etc.), those tests don't pattern-match on Fix today; they should still compile because none of them does an exhaustive match on `DBExpr`. If `cargo build` flags any non-exhaustive match in this file's tests, add a `DBExpr::Fix(_)` arm with a `panic!("unexpected Fix in <test name>")`.

- [ ] **Step 6: Update `src/simplify.rs`**

Two functions match on `Expr` and need Fix arms.

In `occurs` (around line 20), add an arm. The body of `Fix(inner)` is potentially evaluated infinitely many times (each unfold), so a single occurrence inside Fix is "Many" — same conservative treatment as under a lambda:

```rust
Expr::Fix(inner) => match occurs(inner, x) {
    Occ::Zero => Occ::Zero,
    _ => Occ::Many,
},
```

In `step` (around line 43), recurse into the inner without restructuring:

```rust
Expr::Fix(inner) => Expr::fix(step(inner)),
```

- [ ] **Step 7: Update `src/strict.rs`**

`strict.rs` operates on `DBExpr`. Two functions need updates.

In `mark_strict` (around line 36), add an arm in the outer match, between `Abs` and the App/StrictApp catch-all:

```rust
DBExpr::Fix(inner) => DBExpr::fix(mark_strict(inner)),
```

In `head_strict_db` (the analyzer that returns indices forced when reducing to WHNF — find it in the same file), add:

```rust
DBExpr::Fix(inner) => head_strict_db(inner, k),
```

Reasoning: Fix(inner) is unfolded eagerly to `inner (Fix inner)`, and to reach the WHNF of that App, you first force `inner`. So whatever inner forces, Fix(inner) also forces.

- [ ] **Step 8: Verify the crate still compiles**

Run: `cargo build`
Expected: clean build. If any match-arm error appears (`non-exhaustive patterns: &Fix(_) not covered`), add the missing arm at that site — every `match` on `Expr` or `DBExpr` in the codebase needs handling. Common stragglers: helpers in `eval.rs`, additional helpers in `debruijn.rs`, sub-functions in `cbn.rs` test scaffolding.

- [ ] **Step 9: Run all existing tests to verify they still pass**

Run: `cargo test`
Expected: all previously-passing tests still pass. (No new tests yet — they come in Task 10.)

- [ ] **Step 10: Commit**

```bash
git add src/ast.rs src/eval.rs src/simplify.rs src/strict.rs src/debruijn.rs src/cbn.rs src/pretty.rs
git commit -m "ast: add Expr::Fix and thread through all passes"
```

---

### Task 10: Evaluator behavior tests for `fix`

**Files:**
- Modify: `src/eval.rs` (append tests)

- [ ] **Step 1: Write the failing tests**

Append to `src/eval.rs`'s `tests` module:

```rust
#[test]
fn fix_id_unfolds_once() {
    // fix (\x. x)  has no normal form; we verify the step semantics:
    // one reduction step turns it into (\x. x) (fix (\x. x)).
    let e = Expr::fix(Expr::abs("x", Expr::var("x")));
    let stepped = reduce_step(&e).unwrap();
    let expected = Expr::app(
        Expr::abs("x", Expr::var("x")),
        Expr::fix(Expr::abs("x", Expr::var("x"))),
    );
    assert_eq!(stepped, expected);
}

#[test]
fn fix_const_normal_forms_to_const_application() {
    // fix (\x. \y. y)  is equivalent to \y. y after normalization (the
    // body ignores `x`, so further unfoldings do nothing).
    let e = Expr::fix(Expr::abs("x", Expr::abs("y", Expr::var("y"))));
    let nf = normalize(&e, 100).unwrap();
    assert!(alpha_eq(&nf, &Expr::abs("y", Expr::var("y"))));
}

#[test]
fn fix_factorial_zero_returns_one() {
    // Build `fact = fix (\rec. \n. if (isZero n) 1 (mul n (rec (pred n))))`
    // by reusing prelude defs would be too entangled; instead use a tiny
    // direct recursion to test fix evaluator semantics.
    //
    // f = fix (\rec. \n. n)   evaluates to identity (\n. n) after one
    // unfold; applied to 0, returns 0. (\n. n is preserved by repeated
    // fix-unfolds of the const-rec body.)
    let f = Expr::fix(Expr::abs("rec", Expr::abs("n", Expr::var("n"))));
    let zero = Expr::abs("f", Expr::abs("x", Expr::var("x"))); // Church 0
    let applied = Expr::app(f, zero.clone());
    let nf = normalize(&applied, 200).unwrap();
    assert!(alpha_eq(&nf, &zero), "got {:?}", nf);
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib eval::tests::fix`
Expected: 3 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/eval.rs
git commit -m "eval: tests for fix evaluator semantics"
```

---

### Task 11: Parser support for `fix`

**Files:**
- Modify: `src/parser.rs` (reserve `fix`, add atom parser)

- [ ] **Step 1: Write the failing test**

Append to `src/parser.rs`'s `tests` module:

```rust
#[test]
fn parse_fix_simple() {
    // fix (\x. x)  →  Expr::Fix(\x. x)
    assert_eq!(
        parse_expr("fix (\\x. x)").unwrap(),
        Expr::fix(Expr::abs("x", Expr::var("x"))),
    );
}

#[test]
fn parse_fix_in_application() {
    // fix f x  parses as  (fix f) x
    assert_eq!(
        parse_expr("fix f x").unwrap(),
        Expr::app(Expr::fix(Expr::var("f")), Expr::var("x")),
    );
}

#[test]
fn fix_is_reserved_identifier() {
    // `\fix. fix` should fail (fix is reserved as a keyword in atom position).
    // It's OK if this is accepted as long as our test below for top-level
    // binding rejects "def fix = ..." — but cleanest is to reserve.
    assert!(parse_expr("\\fix. fix").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail with the right reason**

Run: `cargo test --lib parser::tests::parse_fix`
Expected: tests fail because `fix` is parsed as an identifier, not as a keyword.

- [ ] **Step 3: Update `src/parser.rs`**

In `ident()`, add `"fix"` to the reserved list:

```rust
if matches!(s.as_str(), "def" | "let" | "in" | "fix") {
    Err(Simple::custom(span, format!("unexpected keyword `{s}`")))
} else {
    Ok(s)
}
```

In `expr_parser()`, add a `fix` atom parser before `let_in`:

```rust
let fix_atom = text::keyword("fix")
    .then_ignore(hws())
    .ignore_then(
        // Parse a single atom (not a full application — `fix` binds tighter
        // than juxtaposition: `fix f x` is `(fix f) x`).
        choice((
            just('(')
                .then_ignore(hws())
                .ignore_then(expr.clone())
                .then_ignore(just(')'))
                .then_ignore(hws()),
            // bare ident as the argument
            ident().map(Expr::Var),
            // bare lambda
            just('\\')
                .then_ignore(hws())
                .ignore_then(ident())
                .then_ignore(just('.'))
                .then_ignore(hws())
                .then(expr.clone())
                .map(|(p, b)| Expr::abs(p, b)),
        ))
    )
    .map(Expr::fix);
```

Add `fix_atom` into the `choice` for `atom`, before `let_in`:

```rust
let atom = choice((fix_atom, let_in, lambda, parens, numeral, var));
```

- [ ] **Step 4: Run tests to verify they now pass**

Run: `cargo test --lib parser::tests`
Expected: all parser tests pass, including the three new `parse_fix*` tests.

- [ ] **Step 5: Commit**

```bash
git add src/parser.rs
git commit -m "parser: fix keyword binding tighter than application"
```

---

### Task 12: Type rule for `Expr::Fix`

**Files:**
- Modify: `src/infer.rs` (add Fix arm to `infer_expr`)

- [ ] **Step 1: Write the failing test**

Append to `src/infer.rs`'s `infer_expr_tests` module:

```rust
#[test]
fn fix_id_has_polymorphic_type() {
    // fix (\x. x)   ⇒   α   (any type — fix-of-id is undefined but well-typed)
    // The point: it does NOT fail occurs-check the way Y does.
    let e = Expr::fix(Expr::abs("x", Expr::var("x")));
    let t = infer(&e).unwrap();
    assert!(matches!(t, Type::Var(_)), "expected a fresh tvar, got {:?}", t);
}

#[test]
fn fix_arg_must_be_endofunction() {
    // fix applied to something whose type isn't α → α should fail.
    // We construct `fix (\x. \y. x)` whose body is α → β → α; unifying with
    // γ → γ forces γ = α and γ = β → α, which is occurs-check infinite.
    let e = Expr::fix(Expr::abs("x", Expr::abs("y", Expr::var("x"))));
    let err = infer(&e);
    assert!(err.is_err(), "expected type error, got {:?}", err);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib infer::infer_expr_tests::fix`
Expected: compile error — `infer_expr` doesn't handle `Fix`.

- [ ] **Step 3: Add the Fix arm**

In `infer_expr`, add (just before the closing `}` of the outer `match`):

```rust
Expr::Fix(inner) => {
    // fix : ∀a. (a → a) → a
    let (s1, t_inner) = infer_expr(env, inner, fresh)?;
    let alpha = fresh.tvar();
    let s2 = unify(&s1.apply(&t_inner), &Type::arrow(alpha.clone(), alpha.clone()))?;
    let composed = s2.compose(&s1);
    Ok((composed.clone(), composed.apply(&alpha)))
}
```

- [ ] **Step 4: Run tests to verify they now pass**

Run: `cargo test --lib infer::infer_expr_tests`
Expected: all pass, including the two new `fix_*` tests.

- [ ] **Step 5: Commit**

```bash
git add src/infer.rs
git commit -m "infer: type rule for Expr::Fix at forall a. (a -> a) -> a"
```

---

## Milestone 4 — Pretty-Printing Types and REPL Integration

End state: REPL prints inferred types after each def and main expression. Type errors print but evaluation runs.

### Task 13: Pretty-print `Type` and `Scheme`

**Files:**
- Modify: `src/types.rs` (impl Display for Type and Scheme)

- [ ] **Step 1: Write the failing test**

Append to `src/types.rs`:

```rust
use std::fmt;

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Number tvars to letters: 0→a, 1→b, ..., 25→z, 26→aa, 27→ab, ...
        // For Display we use raw t<id> to avoid normalization here; the
        // higher-level pretty-printer in Scheme renames into letters.
        match self {
            Type::Var(id) => write!(f, "t{}", id),
            Type::Arrow(a, b) => {
                let a_str = match **a {
                    Type::Arrow(_, _) => format!("({})", a),
                    _ => format!("{}", a),
                };
                write!(f, "{} -> {}", a_str, b)
            }
        }
    }
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.vars.is_empty() {
            return write!(f, "{}", pretty_letters(&self.ty, &[]));
        }
        let pretty_ty = pretty_letters(&self.ty, &self.vars);
        let pretty_vars: Vec<String> = self.vars.iter()
            .map(|v| letter_for_index(self.vars.iter().position(|x| x == v).unwrap()))
            .collect();
        write!(f, "forall {}. {}", pretty_vars.join(" "), pretty_ty)
    }
}

fn letter_for_index(i: usize) -> String {
    // 0→"a" .. 25→"z" .. 26→"aa" 27→"ab" ...
    let mut out = String::new();
    let mut n = i;
    loop {
        let r = (n % 26) as u8;
        out.insert(0, (b'a' + r) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    out
}

fn pretty_letters(t: &Type, bound: &[TVarId]) -> String {
    match t {
        Type::Var(id) => match bound.iter().position(|v| v == id) {
            Some(i) => letter_for_index(i),
            None => format!("t{}", id),
        },
        Type::Arrow(a, b) => {
            let a_str = match **a {
                Type::Arrow(_, _) => format!("({})", pretty_letters(a, bound)),
                _ => pretty_letters(a, bound),
            };
            format!("{} -> {}", a_str, pretty_letters(b, bound))
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn scheme_with_one_quantifier() {
        // ∀a. a → a
        let s = Scheme { vars: vec![0], ty: Type::arrow(Type::var(0), Type::var(0)) };
        assert_eq!(format!("{}", s), "forall a. a -> a");
    }

    #[test]
    fn scheme_with_two_quantifiers() {
        // ∀a b. a → b → a
        let s = Scheme {
            vars: vec![0, 1],
            ty: Type::arrow(Type::var(0), Type::arrow(Type::var(1), Type::var(0))),
        };
        assert_eq!(format!("{}", s), "forall a b. a -> b -> a");
    }

    #[test]
    fn arrow_left_assoc_parens() {
        // (a → a) → a
        let s = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::arrow(Type::var(0), Type::var(0)), Type::var(0)),
        };
        assert_eq!(format!("{}", s), "forall a. (a -> a) -> a");
    }

    #[test]
    fn no_quantifiers_no_forall() {
        let s = Scheme { vars: vec![], ty: Type::var(99) };
        assert_eq!(format!("{}", s), "t99");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib types::display_tests`
Expected: 4 passing tests.

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "types: Display for Type and Scheme with letter renaming"
```

---

### Task 14: REPL prints types alongside evaluation

**Files:**
- Modify: `src/repl.rs` (call `infer_program` before `inline_defs`, print types and errors)

- [ ] **Step 1: Modify `evaluate` in `src/repl.rs`**

Replace the body of `evaluate`:

```rust
fn evaluate(line: &str, env: &mut Vec<Def>) {
    let parsed = match parse_program(line) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    // Build the program-as-typechecked: existing env defs + newly parsed defs.
    let program_for_types = crate::ast::Program {
        defs: env.iter().cloned().chain(parsed.defs.iter().cloned()).collect(),
        main: parsed.main.clone(),
    };
    let types = crate::infer::infer_program(&program_for_types);

    // Print types only for the NEW defs (skip the env prefix).
    let new_count = parsed.defs.len();
    let types_for_new = &types.defs[types.defs.len().saturating_sub(new_count)..];
    for (name, res) in types_for_new {
        match res {
            Ok(scheme) => println!("{} : {}", name, scheme),
            Err(e) => println!("{} : (type error: {})", name, e),
        }
    }

    // Add new defs to env (whether or not they typechecked — advisory mode).
    for d in &parsed.defs {
        env.push(d.clone());
    }

    // Print main type if there is a main.
    if let Some(t_res) = &types.main_type {
        match t_res {
            Ok(t) => {
                // Wrap in scheme for letter-renaming; generalize over its ftv.
                let s = crate::types::Scheme {
                    vars: { let mut v: Vec<_> = t.ftv().into_iter().collect(); v.sort(); v },
                    ty: t.clone(),
                };
                println!(": {}", s);
            }
            Err(e) => println!(": (type error: {})", e),
        }
    }

    // Evaluate main if present (always — advisory mode).
    if let Some(main) = parsed.main {
        let program = Program {
            defs: env.clone(),
            main: Some(main),
        };
        match inline_defs(&program) {
            Ok(e) => {
                let prepared = simplify(&e);
                match normalize(&prepared, STEP_LIMIT) {
                    Ok(nf) => println!("{}", print(&nf)),
                    Err(err) => eprintln!("{}", err),
                }
            }
            Err(err) => eprintln!("{}", err),
        }
    }
}
```

- [ ] **Step 2: Build the project**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 3: Manual smoke test**

Run: `cargo run --quiet -- repl` (or however the REPL is launched — check `main.rs`).

Type at the prompt:
```
\x. x
```
Expected output (something like):
```
: forall a. a -> a
\x. x
```

Type:
```
def myid = \x. x
myid 1
```
Expected:
```
myid : forall a. a -> a
: forall a. (a -> a) -> a -> a   (Church-numeral instantiation)
\f. \x. f x                       (the value, equal to 1)
```

(The exact instantiation may differ based on evaluation order; the test only requires that types are printed and evaluation still runs.)

- [ ] **Step 4: Commit**

```bash
git add src/repl.rs
git commit -m "repl: print inferred types in advisory mode before evaluation"
```

---

### Task 15: File mode prints types

**Files:**
- Modify: `src/main.rs:90` (insert type-checking after `merge`, before `inline_defs`)

- [ ] **Step 1: Locate the insertion point**

Open `src/main.rs`. Around line 90 you'll see:

```rust
let program = merge(load_prelude(), user);

if program.main.is_none() {
    // Library file with only defs — print them and exit.
    ...
}

let inlined = match inline_defs(&program) {
```

We'll insert type-checking between `let program = merge(...)` and the `if program.main.is_none()` branch — types should be reported regardless of whether there's a main.

- [ ] **Step 2: Insert type printing**

Replace the line `let program = merge(load_prelude(), user);` and the subsequent block, up to (but not including) `let inlined = match inline_defs(&program) {`, with:

```rust
let program = merge(load_prelude(), user);

// Type-check (advisory): print types to stderr so stdout remains the
// evaluation result. We typecheck every def, but only print user-supplied
// ones — printing the entire prelude every run would be noise. The
// boundary is `prelude_def_count` from the merged program (prelude defs
// come first; see `merge`).
let prelude_def_count = load_prelude().defs.len();
let types = lc::infer::infer_program(&program);
for (name, res) in types.defs.iter().skip(prelude_def_count) {
    match res {
        Ok(scheme) => eprintln!("{} : {}", name, scheme),
        Err(e) => eprintln!("{} : (type error: {})", name, e),
    }
}
if let Some(t_res) = &types.main_type {
    match t_res {
        Ok(t) => {
            let mut vars: Vec<_> = t.ftv().into_iter().collect();
            vars.sort();
            let s = lc::types::Scheme { vars, ty: t.clone() };
            eprintln!(": {}", s);
        }
        Err(e) => eprintln!(": (type error: {})", e),
    }
}

if program.main.is_none() {
    // Library file with only defs — print them and exit.
    for d in &program.defs {
        println!("def {} = {}", d.name, print(&d.body));
    }
    return;
}
```

(Note: `load_prelude()` is called twice — once when constructing `program`, once for the count. That's a small cost paid only at startup; an alternative is to capture the count before merging. Either approach is fine.)

Add this `use` near the top if not already present:

```rust
use lc::types::Scheme;  // optional — only if you prefer unqualified Scheme
```

- [ ] **Step 3: Build and smoke test**

Run: `cargo run --quiet -- examples/numerals.lc 2>&1 | head -20`
Expected: a few `name : forall ...` lines on stderr; the evaluation result on stdout. The stdout output should be byte-identical to before this task.

Verify:

```bash
cargo run --quiet -- examples/numerals.lc 2>/dev/null > /tmp/new_stdout.txt
git stash
cargo run --quiet -- examples/numerals.lc 2>/dev/null > /tmp/old_stdout.txt
git stash pop
diff /tmp/new_stdout.txt /tmp/old_stdout.txt
```

Expected: no diff.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test`
Expected: all tests pass — file-mode types are emitted on stderr so any stdout-asserting test is unaffected.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "cli: print user-def and main types to stderr in file mode"
```

---

## Milestone 5 — End-to-End Tests

End state: an integration test loads `lib/prelude.lc` and snapshots which defs typecheck. This serves both as regression and as documentation of HM's reach across the existing prelude.

### Task 16: Snapshot test against the real prelude

**Files:**
- Create: `tests/infer_prelude_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! End-to-end: load `lib/prelude.lc`, type-check, and print one line per def.
//! This is intentionally a snapshot/inspection test — it asserts that
//! certain well-known defs typecheck, and prints the rest so a human can
//! review what HM does and doesn't accept.

use std::fs;

use lc::infer::infer_program;
use lc::parser::parse_program;

fn typecheck_status() -> Vec<(String, bool)> {
    let src = fs::read_to_string("lib/prelude.lc").expect("read prelude");
    let program = parse_program(&src).expect("parse prelude");
    let types = infer_program(&program);
    types.defs.into_iter().map(|(n, r)| (n, r.is_ok())).collect()
}

#[test]
fn id_typechecks() {
    let status: std::collections::HashMap<_, _> = typecheck_status().into_iter().collect();
    assert_eq!(status.get("id"), Some(&true));
}

#[test]
fn const_typechecks() {
    let status: std::collections::HashMap<_, _> = typecheck_status().into_iter().collect();
    assert_eq!(status.get("const"), Some(&true));
}

#[test]
fn compose_typechecks() {
    let status: std::collections::HashMap<_, _> = typecheck_status().into_iter().collect();
    assert_eq!(status.get("compose"), Some(&true));
}

#[test]
fn y_combinator_fails_occurs_check() {
    let status: std::collections::HashMap<_, _> = typecheck_status().into_iter().collect();
    // Y is the canonical HM rejection; if this ever flips to true, the type
    // system has changed in a way that needs review.
    assert_eq!(status.get("Y"), Some(&false), "Y must NOT typecheck under HM");
}

#[test]
fn print_full_status() {
    // Diagnostic test — always passes, but prints the full table for review.
    // Run with: cargo test --test infer_prelude_test print_full_status -- --nocapture
    for (name, ok) in typecheck_status() {
        println!("{:>12} : {}", name, if ok { "OK" } else { "TYPE ERROR" });
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --test infer_prelude_test`
Expected: 5 tests pass (4 assertions + 1 print-only).

If any of `id`/`const`/`compose` fails to typecheck, that's a real bug in inference — investigate before changing the assertion.

- [ ] **Step 3: Inspect the diagnostic output**

Run: `cargo test --test infer_prelude_test print_full_status -- --nocapture`
Expected: prints a table like:
```
          id : OK
       const : OK
        flip : OK
     compose : OK
        true : OK
       false : OK
          if : OK
   ...
           Y : TYPE ERROR
        fact : TYPE ERROR  (or OK depending on inference detail)
   ...
```

This is informational. Snapshot what comes out — it's the truth about what HM accepts in your prelude.

- [ ] **Step 4: Commit**

```bash
git add tests/infer_prelude_test.rs
git commit -m "tests: snapshot prelude.lc typecheck status"
```

---

## Done — Acceptance

After all tasks are committed, the following should all be true:

- [ ] `cargo test` is fully green.
- [ ] `cargo run -- repl`, then typing `\x. x` prints `: forall a. a -> a` followed by the value.
- [ ] `cargo run -- repl`, then typing `def Y = \f. (\x. f (x x)) (\x. f (x x))` prints `Y : (type error: occurs check ...)` and continues to accept the def.
- [ ] `cargo run -- repl`, then typing `fix (\rec. \n. n) 5` prints a valid type and reduces to a value.
- [ ] `cargo run -- examples/numerals.lc` produces unchanged stdout; types appear on stderr.
- [ ] `lib/prelude.lc` is unchanged.

## Future work (out of scope)

- **Strict mode (planned next):** make type errors block evaluation; rewrite prelude so `Y` is removed and `fact` uses `fix`; consider primitive `Bool`/`Nat` for clean types on Church encodings.
- **Spans in errors:** thread source positions through the AST so type errors point at the offending sub-expression.
- **Inner `let` generalization:** stop desugaring `let x = e1 in e2` to `App(Abs, _)`; add `Expr::Let` so let-bound variables in nested expressions get HM polymorphism.
- **Type annotations:** allow optional `\x:T. e` so users can pin types when inference yields awkward results.
