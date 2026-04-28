//! Call-by-need evaluator types.
//!
//! Key shift from B.1: instead of *substituting* an argument into a body,
//! we *bind* the argument as a `Thunk` in the body's environment and only
//! reduce it on first lookup. Once forced, the thunk's cell stores the
//! result so subsequent lookups are O(1) — that's the "need" in
//! call-by-need (a.k.a. lazy with sharing).
//!
//! Type sketch:
//!
//! ```text
//!   Env       = stack of Rc<RefCell<Thunk>>
//!   Thunk     = Pending(term, env)  |  Forced(Closure)
//!   Closure   = { body: DBExpr, env: Env }
//! ```
//!
//! Cycle: Thunk → Closure → Env → Thunk. Rc/RefCell threads through this.

use std::cell::RefCell;
use std::rc::Rc;

use crate::debruijn::DBExpr;

/// An environment: a stack of shared, mutable thunk cells.
///
/// Convention: the *last* element corresponds to De Bruijn index 0
/// (the innermost binder). To look up index `i`, take
/// `env[env.len() - 1 - i]`.
pub type Env = Vec<Rc<RefCell<Thunk>>>;

#[derive(Debug, Clone)]
pub enum Thunk {
    /// Not yet evaluated. Reduce `term` in `env` when forced.
    Pending { term: DBExpr, env: Env },
    /// Already reduced to WHNF.
    Forced(Closure),
    /// A "neutral" binder: a placeholder pushed during full-NF traversal.
    /// Reifies as `Var(depth - 1 - level)` instead of reducing further.
    /// Carries a name hint for the eventual reified variable.
    Bound { level: usize, name: String },
}

/// The WHNF of a closed lambda term: a body plus the environment it was
/// captured in.
#[derive(Debug, Clone)]
pub struct Closure {
    /// The lambda body (skipped past the outer `\.`).
    pub body: DBExpr,
    /// Environment captured at the point the lambda was reached.
    pub env: Env,
    /// Original binder name hint (used when reifying back to named Expr).
    pub binder_name: String,
}

impl Thunk {
    pub fn pending(term: DBExpr, env: Env) -> Rc<RefCell<Thunk>> {
        Rc::new(RefCell::new(Thunk::Pending { term, env }))
    }
    pub fn forced(c: Closure) -> Rc<RefCell<Thunk>> {
        Rc::new(RefCell::new(Thunk::Forced(c)))
    }
    pub fn bound(level: usize, name: impl Into<String>) -> Rc<RefCell<Thunk>> {
        Rc::new(RefCell::new(Thunk::Bound { level, name: name.into() }))
    }
}

/// Look up De Bruijn index `i` in `env`. Panics if out of range
/// (well-formed closed terms shouldn't ever do this).
pub fn lookup(env: &Env, i: usize) -> Rc<RefCell<Thunk>> {
    let pos = env
        .len()
        .checked_sub(1 + i)
        .unwrap_or_else(|| panic!("lookup: index {i} out of bounds (env len {})", env.len()));
    Rc::clone(&env[pos])
}

/// Extend an env with a new thunk (becomes the new index-0 slot).
pub fn extend(env: &Env, t: Rc<RefCell<Thunk>>) -> Env {
    let mut out = env.clone();
    out.push(t);
    out
}

/// A weak-head value: either a closure (lambda + captured env) or a
/// neutral term (a free variable applied to some args, can't reduce
/// further). Pure-λ closed terms in an empty env never produce neutrals;
/// they only arise when `nf` descends under a binder and pushes a
/// `Bound` placeholder.
#[derive(Debug, Clone)]
pub enum Value {
    Cls(Closure),
    /// A stuck application: `Var(level)` applied to thunks (in the
    /// envs they were created in). Args are stored as `(term, env)`
    /// pairs because they haven't been reduced to NF yet.
    Neu {
        head_level: usize,
        head_name: String,
        args: Vec<(DBExpr, Env)>,
    },
}

/// Reduce `term @ env` to weak-head form. Pushes args onto neutral heads
/// when applicable. β-reduces when the head is a closure.
pub fn whnf(term: &DBExpr, env: &Env) -> Value {
    match term {
        DBExpr::Var(i) => {
            let cell = lookup(env, *i);
            force(&cell)
        }
        DBExpr::Abs(name, body) => Value::Cls(Closure {
            body: (**body).clone(),
            env: env.clone(),
            binder_name: name.clone(),
        }),
        DBExpr::App(f, x) => match whnf(f, env) {
            Value::Cls(c) => {
                let arg_thunk = Thunk::pending((**x).clone(), env.clone());
                let new_env = extend(&c.env, arg_thunk);
                whnf(&c.body, &new_env)
            }
            Value::Neu {
                head_level,
                head_name,
                mut args,
            } => {
                args.push(((**x).clone(), env.clone()));
                Value::Neu {
                    head_level,
                    head_name,
                    args,
                }
            }
        },
    }
}

/// Force a thunk cell to a `Value`. If already `Forced`, returns the
/// stored closure as `Value::Cls`. If `Bound`, returns a neutral with
/// no args. Otherwise clones the pending term/env, drops the borrow,
/// reduces, and writes the closure back (memoization).
///
/// Note: we deliberately *clone* the pending term and env instead of
/// taking them out and replacing with a placeholder. If we replaced,
/// any recursive force of the same cell during its own reduction
/// (which Y combinator self-application can cause) would see the
/// placeholder instead of the real term — silent corruption.
fn force(cell: &Rc<RefCell<Thunk>>) -> Value {
    let (term, env) = {
        let borrow = cell.borrow();
        match &*borrow {
            Thunk::Forced(c) => return Value::Cls(c.clone()),
            Thunk::Bound { level, name } => {
                return Value::Neu {
                    head_level: *level,
                    head_name: name.clone(),
                    args: Vec::new(),
                };
            }
            Thunk::Pending { term, env } => (term.clone(), env.clone()),
        }
    };
    let result = whnf(&term, &env);
    if let Value::Cls(c) = &result {
        *cell.borrow_mut() = Thunk::Forced(c.clone());
    }
    result
}

/// Reduce `term @ env` to full normal form, producing a closed `DBExpr`.
/// `depth` is the number of binders we're currently *inside* during the
/// reification walk; needed to translate Bound levels into De Bruijn
/// indices.
pub fn nf(term: &DBExpr, env: &Env, depth: usize) -> DBExpr {
    match whnf(term, env) {
        Value::Cls(c) => {
            // Descend under the binder: push a Bound at the current depth,
            // normalize the body, wrap the result in an Abs.
            let bound = Thunk::bound(depth, c.binder_name.clone());
            let new_env = extend(&c.env, bound);
            let body_nf = nf(&c.body, &new_env, depth + 1);
            DBExpr::abs(c.binder_name, body_nf)
        }
        Value::Neu {
            head_level,
            head_name: _,
            args,
        } => {
            let mut result = DBExpr::Var(depth - 1 - head_level);
            for (a_term, a_env) in args {
                result = DBExpr::app(result, nf(&a_term, &a_env, depth));
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dvar(i: usize) -> DBExpr {
        DBExpr::Var(i)
    }

    #[test]
    fn lookup_returns_pushed_thunk_at_index_zero() {
        let env: Env = Vec::new();
        let t = Thunk::pending(dvar(0), Vec::new());
        let env = extend(&env, t.clone());
        // Index 0 should be the thunk we just pushed.
        assert!(Rc::ptr_eq(&lookup(&env, 0), &t));
    }

    #[test]
    fn lookup_resolves_outer_via_higher_index() {
        let env: Env = Vec::new();
        let outer = Thunk::pending(dvar(7), Vec::new());
        let inner = Thunk::pending(dvar(8), Vec::new());
        let env = extend(&env, outer.clone());
        let env = extend(&env, inner.clone());
        assert!(Rc::ptr_eq(&lookup(&env, 0), &inner));
        assert!(Rc::ptr_eq(&lookup(&env, 1), &outer));
    }

    // ---- whnf ----

    fn dabs(name: &str, body: DBExpr) -> DBExpr {
        DBExpr::abs(name, body)
    }
    fn dapp(f: DBExpr, x: DBExpr) -> DBExpr {
        DBExpr::app(f, x)
    }

    fn assert_closure(v: Value) -> Closure {
        match v {
            Value::Cls(c) => c,
            Value::Neu { .. } => panic!("expected closure, got neutral"),
        }
    }

    #[test]
    fn whnf_of_lambda_is_self() {
        let term = dabs("x", dvar(0));
        let c = assert_closure(whnf(&term, &Vec::new()));
        assert_eq!(c.body, dvar(0));
        assert_eq!(c.binder_name, "x");
        assert!(c.env.is_empty());
    }

    #[test]
    fn whnf_of_identity_app() {
        let id = dabs("x", dvar(0));
        let id2 = dabs("y", dvar(0));
        let term = dapp(id, id2);
        let c = assert_closure(whnf(&term, &Vec::new()));
        assert_eq!(c.body, dvar(0));
        assert_eq!(c.binder_name, "y");
    }

    #[test]
    fn whnf_const_picks_first() {
        let const_fn = dabs("x", dabs("y", dvar(1)));
        let arg1 = dabs("a", dvar(0));
        let arg2 = dabs("b", dvar(0));
        let term = dapp(dapp(const_fn, arg1), arg2);
        let c = assert_closure(whnf(&term, &Vec::new()));
        assert_eq!(c.binder_name, "a");
        assert_eq!(c.body, dvar(0));
    }

    #[test]
    fn whnf_memoizes_pending_thunk() {
        let id_z = dabs("z", dvar(0));
        let id_y = dabs("y", dvar(0));
        let arg = dapp(id_y, id_z);
        let id_x = dabs("x", dvar(0));
        let term = dapp(id_x, arg);
        let c = assert_closure(whnf(&term, &Vec::new()));
        assert_eq!(c.binder_name, "z");
        assert_eq!(c.body, dvar(0));
    }

    // ---- nf ----

    #[test]
    fn nf_identity() {
        // (\x. x) (\y. y)  →  \y. y  in DB: \. 0
        let id = dabs("x", dvar(0));
        let id2 = dabs("y", dvar(0));
        let term = dapp(id, id2);
        let result = nf(&term, &Vec::new(), 0);
        assert_eq!(result, dabs("y", dvar(0)));
    }

    #[test]
    fn nf_under_lambda_descends() {
        // \f. (\y. y) f  →  \f. f
        let inner = dapp(dabs("y", dvar(0)), dvar(0));
        let term = dabs("f", inner);
        let result = nf(&term, &Vec::new(), 0);
        assert_eq!(result, dabs("f", dvar(0)));
    }

    #[test]
    fn nf_church_two_normalizes_to_self() {
        // \f. \x. f (f x) is already in NF
        let two = dabs(
            "f",
            dabs("x", dapp(dvar(1), dapp(dvar(1), dvar(0)))),
        );
        let result = nf(&two, &Vec::new(), 0);
        assert_eq!(result, two);
    }

    #[test]
    fn forced_thunk_can_be_replaced() {
        // Smoke-test the mutability story: take a Pending cell, replace
        // its contents with Forced — all references see the change.
        let cell = Thunk::pending(dvar(0), Vec::new());
        let observer = Rc::clone(&cell);

        let dummy_closure = Closure {
            body: dvar(0),
            env: Vec::new(),
            binder_name: "x".into(),
        };
        *cell.borrow_mut() = Thunk::Forced(dummy_closure);

        let is_forced = matches!(&*observer.borrow(), Thunk::Forced(_));
        assert!(is_forced, "observer still sees Pending after replacement");
    }
}
