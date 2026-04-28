//! De Bruijn–indexed lambda calculus IR.
//!
//! Variables become non-negative integers counting outward to the binding
//! lambda. Binders carry a name hint (purely cosmetic — used by
//! `to_named` to preserve the original term's binder names). The hint is
//! ignored by equality, so two terms are α-equivalent iff they're `==`.
//!
//! Conversion shape:
//!   Named `Expr`  --to_db-->  `DBExpr`  --to_named-->  Named `Expr`
//!
//! Round-trip is name-preserving for un-reduced binders; β-reduction
//! consumes the redex's binder name, but inner binders keep their hints.

use std::rc::Rc;

use crate::ast::Expr;

/// Children are stored in `Rc` so cloning a `DBExpr` is O(1) (modulo
/// the binder-name `String` clone in Abs). Hot paths in `cbn` depend on
/// cheap clones — see force/whnf, which clone the pending term every
/// time it's forced.
#[derive(Debug, Clone)]
pub enum DBExpr {
    /// De Bruijn index: number of lambdas to walk *outward* before hitting
    /// the binder. Index 0 is the immediately enclosing lambda. Indices
    /// `>= depth` are free (shouldn't appear after `inline_defs`).
    Var(usize),
    /// Binder. The `String` is a name hint for pretty-printing only —
    /// equality ignores it.
    Abs(String, Rc<DBExpr>),
    App(Rc<DBExpr>, Rc<DBExpr>),
    /// Like `App`, but the runtime evaluates the argument eagerly to WHNF
    /// before binding. Inserted by `mark_strict` at App-spine positions
    /// where strictness analysis proves the binder will be forced.
    /// Semantically equivalent to `App`; differs only in evaluation order.
    StrictApp(Rc<DBExpr>, Rc<DBExpr>),
}

impl PartialEq for DBExpr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DBExpr::Var(a), DBExpr::Var(b)) => a == b,
            (DBExpr::Abs(_, a), DBExpr::Abs(_, b)) => a == b,
            (DBExpr::App(f1, x1), DBExpr::App(f2, x2)) => f1 == f2 && x1 == x2,
            // Strictness is an evaluation hint; ignore it for structural
            // equality so α-equivalence is unaffected by mark_strict.
            (DBExpr::StrictApp(f1, x1), DBExpr::StrictApp(f2, x2)) => f1 == f2 && x1 == x2,
            (DBExpr::App(f1, x1), DBExpr::StrictApp(f2, x2))
            | (DBExpr::StrictApp(f1, x1), DBExpr::App(f2, x2)) => f1 == f2 && x1 == x2,
            _ => false,
        }
    }
}
impl Eq for DBExpr {}

impl DBExpr {
    pub fn var(i: usize) -> Self {
        DBExpr::Var(i)
    }
    pub fn abs(name: impl Into<String>, body: DBExpr) -> Self {
        DBExpr::Abs(name.into(), Rc::new(body))
    }
    pub fn app(f: DBExpr, x: DBExpr) -> Self {
        DBExpr::App(Rc::new(f), Rc::new(x))
    }
    pub fn strict_app(f: DBExpr, x: DBExpr) -> Self {
        DBExpr::StrictApp(Rc::new(f), Rc::new(x))
    }
}

/// Shift all *free* indices in `e` by `d`, where "free" means `>= cutoff`.
///
/// Used by `subst` (step 3) to fix up indices when a term is moved across
/// binders. With cutoff `c` set to the current binder depth, indices `< c`
/// are bound *within* `e` and stay put; indices `>= c` reach outside `e`
/// and need to be rewritten.
///
/// `d` is signed: positive shifts increase indices (used when descending
/// under fresh binders), negative shifts decrease them (used after a β-
/// reduction to "close the gap" left by the consumed binder).
pub fn shift(d: i64, cutoff: usize, e: &DBExpr) -> DBExpr {
    match e {
        DBExpr::Var(k) => {
            if *k < cutoff {
                DBExpr::var(*k)
            } else {
                let new_k = (*k as i64) + d;
                if new_k < 0 {
                    panic!("shift produced a negative index ({new_k}) — this is a bug");
                }
                DBExpr::var(new_k as usize)
            }
        }
        DBExpr::Abs(name, body) => DBExpr::abs(name.clone(), shift(d, cutoff + 1, body)),
        DBExpr::App(f, x) => DBExpr::app(shift(d, cutoff, f), shift(d, cutoff, x)),
        DBExpr::StrictApp(f, x) => {
            DBExpr::strict_app(shift(d, cutoff, f), shift(d, cutoff, x))
        }
    }
}

/// One step of leftmost-outermost (normal-order) reduction over `DBExpr`.
/// Returns `None` if `e` is in normal form.
///
/// β-redex (`(\. M) N`) reduces to `shift(-1) (subst(0, shift(1, N), M))`:
///   1. shift N up by 1 — it's about to live one binder deeper
///   2. substitute it for index 0 in M
///   3. shift everything down by 1 — the consumed binder is gone
pub fn reduce_step(e: &DBExpr) -> Option<DBExpr> {
    match e {
        DBExpr::App(f, a) | DBExpr::StrictApp(f, a) => {
            if let DBExpr::Abs(_, body) = &**f {
                let lifted_a = shift(1, 0, a);
                let substituted = subst(0, &lifted_a, body);
                Some(shift(-1, 0, &substituted))
            } else if let Some(f2) = reduce_step(f) {
                Some(DBExpr::app(f2, (**a).clone()))
            } else {
                reduce_step(a).map(|a2| DBExpr::app((**f).clone(), a2))
            }
        }
        DBExpr::Abs(name, body) => {
            reduce_step(body).map(|b| DBExpr::abs(name.clone(), b))
        }
        _ => None,
    }
}

/// Substitute `s` for free index `k` in `e`. Notation: `[k -> s] e`.
///
/// When we descend through `n` binders, the slot we're targeting becomes
/// `k + n`, and the replacement `s` must be shifted by `n` so its own
/// free indices still point to the same binders they did originally.
pub fn subst(k: usize, s: &DBExpr, e: &DBExpr) -> DBExpr {
    match e {
        DBExpr::Var(j) => {
            if *j == k {
                shift(k as i64, 0, s)
            } else {
                DBExpr::var(*j)
            }
        }
        DBExpr::Abs(name, body) => DBExpr::abs(name.clone(), subst(k + 1, s, body)),
        DBExpr::App(f, x) => DBExpr::app(subst(k, s, f), subst(k, s, x)),
        DBExpr::StrictApp(f, x) => {
            DBExpr::strict_app(subst(k, s, f), subst(k, s, x))
        }
    }
}

/// Convert a named `Expr` to its De Bruijn form.
///
/// Walks the term while threading an env (stack of binder names from outer
/// to inner). At each `Var(name)`, find the *innermost* matching binder
/// (`rposition`); the De Bruijn index is `depth - 1 - position`.
///
/// Panics on free variables. Inputs should be closed terms — call this
/// after `inline_defs`.
pub fn to_db(e: &Expr) -> DBExpr {
    fn go(e: &Expr, env: &mut Vec<String>) -> DBExpr {
        match e {
            Expr::Var(name) => {
                let i = env
                    .iter()
                    .rposition(|n| n == name)
                    .unwrap_or_else(|| panic!("free variable in to_db: {name}"));
                DBExpr::var(env.len() - 1 - i)
            }
            Expr::Abs(p, body) => {
                env.push(p.clone());
                let body_db = go(body, env);
                env.pop();
                DBExpr::abs(p.clone(), body_db)
            }
            Expr::App(f, x) => DBExpr::app(go(f, env), go(x, env)),
        }
    }
    go(e, &mut Vec::new())
}

/// Convert a De Bruijn term back to a named `Expr` for printing/inspection.
///
/// Walks with a depth counter starting at 0. Each binder we descend into
/// gets a synthesized name `x{depth}` (so the outermost binder is `x0`,
/// next `x1`, etc.). When we hit `Var(i)` at depth `d`, the binder it
/// refers to was created at depth `d - 1 - i`, so its name is `x{d-1-i}`.
///
/// Panics on free indices.
pub fn to_named(e: &DBExpr) -> Expr {
    // Iterative two-stack walk (work / done) so deep DB trees don't blow
    // the Rust call stack. Same pattern as `cbn::nf`. The `env` vec tracks
    // binder names visible at the current scope; we push on Abs descent
    // and pop after BuildAbs.
    enum Step<'a> {
        Process(&'a DBExpr),
        BuildAbs(String),
        BuildApp,
    }

    let mut work: Vec<Step> = vec![Step::Process(e)];
    let mut done: Vec<Expr> = Vec::new();
    let mut env: Vec<String> = Vec::new();

    while let Some(step) = work.pop() {
        match step {
            Step::Process(e) => match e {
                DBExpr::Var(i) => {
                    let pos = env
                        .len()
                        .checked_sub(1 + *i)
                        .unwrap_or_else(|| panic!("free index in to_named: {i}"));
                    done.push(Expr::var(env[pos].clone()));
                }
                DBExpr::Abs(name, body) => {
                    let mut unique: String = (**name).to_string();
                    while env.contains(&unique) {
                        unique.push('\'');
                    }
                    env.push(unique.clone());
                    work.push(Step::BuildAbs(unique));
                    work.push(Step::Process(body));
                }
                DBExpr::App(f, x) | DBExpr::StrictApp(f, x) => {
                    work.push(Step::BuildApp);
                    // Order so f's result is below x's on the done stack —
                    // we'll pop x first, then f, then build App(f, x).
                    work.push(Step::Process(x));
                    work.push(Step::Process(f));
                }
            },
            Step::BuildAbs(name) => {
                env.pop();
                let body = done.pop().expect("to_named: BuildAbs missing body");
                done.push(Expr::abs(name, body));
            }
            Step::BuildApp => {
                // x was pushed last (Process(x) below Process(f)), so
                // x's NF is on top of `done`; f's is below.
                let x = done.pop().expect("to_named: BuildApp missing x");
                let f = done.pop().expect("to_named: BuildApp missing f");
                done.push(Expr::app(f, x));
            }
        }
    }

    debug_assert_eq!(done.len(), 1);
    done.pop().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::eval::alpha_eq;

    // Tiny helpers to make the test data readable.
    fn dvar(i: usize) -> DBExpr {
        DBExpr::var(i)
    }
    // Test helper: name doesn't matter for equality (custom PartialEq
    // ignores it), so all test data uses "_".
    fn dabs(body: DBExpr) -> DBExpr {
        DBExpr::abs("_", body)
    }
    fn dapp(f: DBExpr, x: DBExpr) -> DBExpr {
        DBExpr::app(f, x)
    }

    // ---- to_db ----

    #[test]
    fn to_db_identity() {
        // \x. x  →  \. 0
        let e = Expr::abs("x", Expr::var("x"));
        assert_eq!(to_db(&e), dabs(dvar(0)));
    }

    #[test]
    fn to_db_const_picks_outer() {
        // \x. \y. x  →  \. \. 1
        let e = Expr::abs("x", Expr::abs("y", Expr::var("x")));
        assert_eq!(to_db(&e), dabs(dabs(dvar(1))));
    }

    #[test]
    fn to_db_second_picks_inner() {
        // \x. \y. y  →  \. \. 0
        let e = Expr::abs("x", Expr::abs("y", Expr::var("y")));
        assert_eq!(to_db(&e), dabs(dabs(dvar(0))));
    }

    #[test]
    fn to_db_church_two() {
        // \f. \x. f (f x)  →  \. \. 1 (1 0)
        let e = Expr::abs(
            "f",
            Expr::abs(
                "x",
                Expr::app(Expr::var("f"), Expr::app(Expr::var("f"), Expr::var("x"))),
            ),
        );
        let expected = dabs(dabs(dapp(dvar(1), dapp(dvar(1), dvar(0)))));
        assert_eq!(to_db(&e), expected);
    }

    #[test]
    fn to_db_alpha_equivalent_terms_collide() {
        // \x. x and \y. y both convert to \. 0 — α-equivalence is now ==
        let a = Expr::abs("x", Expr::var("x"));
        let b = Expr::abs("y", Expr::var("y"));
        assert_eq!(to_db(&a), to_db(&b));
    }

    #[test]
    fn to_db_innermost_shadowing() {
        // \x. \x. x  →  \. \. 0  (innermost x wins)
        let e = Expr::abs("x", Expr::abs("x", Expr::var("x")));
        assert_eq!(to_db(&e), dabs(dabs(dvar(0))));
    }

    // ---- to_named ----

    #[test]
    fn to_named_identity() {
        // \"_". 0  →  \_. _   (uses the stored binder name)
        let d = dabs(dvar(0));
        let expected = Expr::abs("_", Expr::var("_"));
        assert_eq!(to_named(&d), expected);
    }

    #[test]
    fn to_named_uses_stored_names() {
        // Build a DB term with explicit binder names, confirm to_named uses them.
        let d = DBExpr::abs(
            "f",
            DBExpr::abs(
                "x",
                DBExpr::app(DBExpr::var(1), DBExpr::var(0)),
            ),
        );
        let expected = Expr::abs("f", Expr::abs("x", Expr::app(Expr::var("f"), Expr::var("x"))));
        assert_eq!(to_named(&d), expected);
    }

    // ---- round trip ----

    #[test]
    fn round_trip_preserves_alpha_equivalence() {
        let originals = vec![
            Expr::abs("x", Expr::var("x")),
            Expr::abs("x", Expr::abs("y", Expr::var("x"))),
            Expr::abs(
                "f",
                Expr::abs(
                    "x",
                    Expr::app(Expr::var("f"), Expr::app(Expr::var("f"), Expr::var("x"))),
                ),
            ),
        ];
        for e in originals {
            let round = to_named(&to_db(&e));
            assert!(
                alpha_eq(&e, &round),
                "round-trip changed term:\n  before: {:?}\n  after:  {:?}",
                e,
                round,
            );
        }
    }

    // ---- reduce_step ----

    #[test]
    fn step_identity_applied_to_identity() {
        // (\. 0) (\. 0)  →  \. 0
        let e = dapp(dabs(dvar(0)), dabs(dvar(0)));
        assert_eq!(reduce_step(&e), Some(dabs(dvar(0))));
    }

    #[test]
    fn step_no_redex_returns_none() {
        // \. 0 has no redex
        assert_eq!(reduce_step(&dabs(dvar(0))), None);
    }

    #[test]
    fn step_const_picks_first() {
        // (\. \. 1) X Y  -- but App is binary; build (\.\.1) X first, then apply Y.
        // Actually we want: ((\. \. 1) X) Y  which is two β-steps.
        // After first step: (\. shifted-X) Y where the "1" became X (after gap-close).
        //
        // Trickier to write inline; test the simpler "two-arg pick first" via
        // multi-step in a higher test. Here just check one β:
        // (\. \. 0) X  →  \. 0   (the X is discarded since the binder we consume
        //                          isn't referenced; well actually here 0 is the
        //                          *inner* binder, so it IS used).
        // Easier: (\. 0) X  →  X
        let x = dabs(dvar(0));
        let e = dapp(dabs(dvar(0)), x.clone());
        assert_eq!(reduce_step(&e), Some(x));
    }

    #[test]
    fn step_under_lambda() {
        // \. (\. 0) 0  →  \. 0
        // The body is a redex; reduce inside the binder.
        let inner = dapp(dabs(dvar(0)), dvar(0));
        let e = dabs(inner);
        assert_eq!(reduce_step(&e), Some(dabs(dvar(0))));
    }

    // ---- subst ----

    #[test]
    fn subst_replaces_matching_var() {
        assert_eq!(subst(0, &dvar(7), &dvar(0)), dvar(7));
    }

    #[test]
    fn subst_leaves_non_matching_var() {
        assert_eq!(subst(0, &dvar(7), &dvar(1)), dvar(1));
    }

    #[test]
    fn subst_under_abs_shifts_replacement() {
        // [0 -> Var(7)] in (\. 1) = (\. 8)
        let e = dabs(dvar(1));
        assert_eq!(subst(0, &dvar(7), &e), dabs(dvar(8)));
    }

    #[test]
    fn subst_under_abs_leaves_inner_binder_alone() {
        // [0 -> Var(7)] in (\. 0) = (\. 0)
        let e = dabs(dvar(0));
        assert_eq!(subst(0, &dvar(7), &e), dabs(dvar(0)));
    }

    #[test]
    fn subst_app_recurses_both_halves() {
        let e = dapp(dvar(0), dvar(0));
        assert_eq!(subst(0, &dvar(9), &e), dapp(dvar(9), dvar(9)));
    }

    // ---- shift ----

    #[test]
    fn shift_var_below_cutoff_unchanged() {
        assert_eq!(shift(5, 1, &dvar(0)), dvar(0));
    }

    #[test]
    fn shift_var_at_or_above_cutoff_increased() {
        assert_eq!(shift(5, 1, &dvar(1)), dvar(6));
        assert_eq!(shift(5, 1, &dvar(2)), dvar(7));
    }

    #[test]
    fn shift_under_abs_lifts_cutoff() {
        // \. 0 — index 0 is bound by the abs; cutoff inside is 1
        assert_eq!(shift(2, 0, &dabs(dvar(0))), dabs(dvar(0)));
        // \. 1 — index 1 was free; lifted by 2
        assert_eq!(shift(2, 0, &dabs(dvar(1))), dabs(dvar(3)));
    }

    #[test]
    fn shift_app_recurses_both_halves() {
        let e = dapp(dvar(0), dvar(1));
        let expected = dapp(dvar(1), dvar(2));
        assert_eq!(shift(1, 0, &e), expected);
    }

    #[test]
    fn shift_negative_decreases() {
        assert_eq!(shift(-1, 0, &dvar(3)), dvar(2));
    }

    #[test]
    fn round_trip_db_with_distinct_names() {
        // Going DB → named → DB preserves structure when binder name hints
        // are distinct (no shadowing collisions during the named round-trip).
        let originals = vec![
            DBExpr::abs("a", dvar(0)),
            DBExpr::abs("a", DBExpr::abs("b", dvar(1))),
            DBExpr::abs(
                "f",
                DBExpr::abs(
                    "x",
                    dapp(dvar(1), dapp(dvar(1), dvar(0))),
                ),
            ),
        ];
        for d in originals {
            let round = to_db(&to_named(&d));
            assert_eq!(d, round);
        }
    }
}
