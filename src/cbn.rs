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
use crate::error::EvalError;

/// Reduction budget. Tick on each β-reduction and each pending-thunk
/// force; when exhausted, callers return `EvalError::StepLimitExceeded`.
pub struct Budget {
    limit: usize,
    used: usize,
}

impl Budget {
    pub fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }
    pub fn tick(&mut self) -> Result<(), EvalError> {
        if self.used >= self.limit {
            Err(EvalError::StepLimitExceeded(self.limit))
        } else {
            self.used += 1;
            Ok(())
        }
    }
}

/// A persistent stack of shared, mutable thunk cells. Cons-list shape
/// (`Some(Rc<EnvNode>)` = non-empty, `None` = empty). Clone is O(1)
/// (just an `Rc` bump); extend is O(1) (cons one node onto the front).
/// Lookup of index `i` is O(i) — fine because typical λ programs
/// don't reach deep through the env.
pub type Env = Option<Rc<EnvNode>>;

#[derive(Debug)]
pub struct EnvNode {
    pub head: Rc<RefCell<Thunk>>,
    pub tail: Env,
}

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

/// Look up De Bruijn index `i` in `env`. Walks `i` cons-cells deep.
/// Panics if out of range (well-formed closed terms shouldn't ever do this).
pub fn lookup(env: &Env, i: usize) -> Rc<RefCell<Thunk>> {
    let mut node = env.as_ref().unwrap_or_else(|| {
        panic!("lookup: index {i} out of bounds (empty env)")
    });
    let mut remaining = i;
    while remaining > 0 {
        node = node.tail.as_ref().unwrap_or_else(|| {
            panic!("lookup: index {i} out of bounds (env too shallow)")
        });
        remaining -= 1;
    }
    Rc::clone(&node.head)
}

/// Extend an env with a new thunk (becomes the new index-0 slot).
/// O(1): just cons a node onto the front.
pub fn extend(env: &Env, t: Rc<RefCell<Thunk>>) -> Env {
    Some(Rc::new(EnvNode {
        head: t,
        tail: env.clone(),
    }))
}

pub fn empty_env() -> Env {
    None
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

/// A frame on the Krivine work-stack.
enum Frame {
    /// Pending application argument. When a closure becomes the focus,
    /// pop this and β-reduce.
    Arg(DBExpr, Env),
    /// Memoization marker. When a closure becomes the focus, write it
    /// back into this cell (so subsequent forces are O(1)).
    Update(Rc<RefCell<Thunk>>),
}

/// Reduce `term @ env` to weak-head form using an iterative Krivine-style
/// machine. Returns `StepLimitExceeded` when the budget is exhausted.
///
/// State is `(focus, env, stack)`. Each loop iteration applies one
/// transition. The Rust call stack stays flat; the work-stack is
/// explicit on the heap.
pub fn whnf(term: &DBExpr, env: &Env, budget: &mut Budget) -> Result<Value, EvalError> {
    let mut focus: DBExpr = term.clone();
    let mut env: Env = env.clone();
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        match focus {
            DBExpr::App(f, x) => {
                // Push the arg (in current env) and shift focus to the function.
                stack.push(Frame::Arg((*x).clone(), env.clone()));
                focus = (*f).clone();
            }
            DBExpr::Abs(name, body) => {
                // We have a value (a closure). Dispatch on the stack.
                let closure = Closure {
                    body: (*body).clone(),
                    env: env.clone(),
                    binder_name: name,
                };
                loop {
                    match stack.pop() {
                        Some(Frame::Arg(arg_term, arg_env)) => {
                            // β-reduce: bind arg as a thunk in closure's env.
                            budget.tick()?;
                            let arg_thunk = Thunk::pending(arg_term, arg_env);
                            env = extend(&closure.env, arg_thunk);
                            focus = closure.body;
                            break; // back to outer loop
                        }
                        Some(Frame::Update(cell)) => {
                            // Memoize: subsequent forces of `cell` are O(1).
                            *cell.borrow_mut() = Thunk::Forced(closure.clone());
                            // Continue popping — the same closure is the
                            // result for any further Update frames or for
                            // an underlying Arg / empty stack.
                        }
                        None => {
                            // Stack empty → done.
                            return Ok(Value::Cls(closure));
                        }
                    }
                }
            }
            DBExpr::Var(i) => {
                let cell = lookup(&env, i);
                let action = {
                    let b = cell.borrow();
                    match &*b {
                        Thunk::Forced(c) => Action::Forced(c.clone()),
                        Thunk::Pending { term, env } => Action::Pending(term.clone(), env.clone()),
                        Thunk::Bound { level, name } => Action::Bound(*level, name.clone()),
                    }
                };
                match action {
                    Action::Forced(c) => {
                        // Treat as if focus were Abs of c — fall through to
                        // the same dispatch logic by re-emitting the Abs.
                        focus = DBExpr::abs(c.binder_name.clone(), c.body.clone());
                        env = c.env.clone();
                    }
                    Action::Pending(term, env_p) => {
                        budget.tick()?;
                        stack.push(Frame::Update(Rc::clone(&cell)));
                        focus = term;
                        env = env_p;
                    }
                    Action::Bound(level, name) => {
                        // Build a neutral. Pop Args off the stack into the
                        // neutral's arg list. Discard any Update frames —
                        // we can't memoize a neutral as a closure.
                        let mut args: Vec<(DBExpr, Env)> = Vec::new();
                        while let Some(frame) = stack.pop() {
                            match frame {
                                Frame::Arg(t, e) => args.push((t, e)),
                                Frame::Update(_) => {}
                            }
                        }
                        return Ok(Value::Neu {
                            head_level: level,
                            head_name: name,
                            args,
                        });
                    }
                }
            }
        }
    }
}

/// What to do with a thunk cell after inspecting it. Local to `whnf`.
enum Action {
    Forced(Closure),
    Pending(DBExpr, Env),
    Bound(usize, String),
}

/// Reduce `term @ env` to full normal form, producing a closed `DBExpr`.
/// `depth` is the number of binders we're currently *inside* during the
/// reification walk; needed to translate Bound levels into De Bruijn
/// indices.
/// Iterative full normal form using two heap-allocated stacks (`work` of
/// pending steps, `done` of finished sub-results). Each leaf-recursive call
/// in the equivalent tree-recursive version becomes a `Process` step; each
/// "after the recursion, combine" continuation becomes a `BuildAbs` /
/// `BuildNeutral` step.
///
/// The Rust call stack stays flat regardless of output tree depth — useful
/// for things like factorial that produce thousand-deep App chains.
pub fn nf(
    term: &DBExpr,
    env: &Env,
    depth: usize,
    budget: &mut Budget,
) -> Result<DBExpr, EvalError> {
    enum Step {
        /// Reduce `(term, env)` at `depth` and dispatch on the resulting Value.
        Process(DBExpr, Env, usize),
        /// `done` already has a body; pop it and wrap as `Abs(name, body)`.
        BuildAbs(String),
        /// `done` already has `k` args; pop them, build the neutral
        /// `Var(depth - 1 - head_level) <args>`.
        BuildNeutral { head_level: usize, k: usize, depth: usize },
    }

    let mut work: Vec<Step> = vec![Step::Process(term.clone(), env.clone(), depth)];
    let mut done: Vec<DBExpr> = Vec::new();

    while let Some(step) = work.pop() {
        match step {
            Step::Process(term, env, depth) => match whnf(&term, &env, budget)? {
                Value::Cls(c) => {
                    let name = c.binder_name.clone();
                    let bound = Thunk::bound(depth, name.clone());
                    let new_env = extend(&c.env, bound);
                    // Push BuildAbs FIRST so it runs after the body is done.
                    work.push(Step::BuildAbs(name));
                    work.push(Step::Process(c.body, new_env, depth + 1));
                }
                Value::Neu {
                    head_level,
                    head_name: _,
                    args,
                } => {
                    let k = args.len();
                    work.push(Step::BuildNeutral { head_level, k, depth });
                    // Push args in reverse so they pop in original order.
                    for (a_term, a_env) in args.into_iter().rev() {
                        work.push(Step::Process(a_term, a_env, depth));
                    }
                }
            },
            Step::BuildAbs(name) => {
                let body_nf = done.pop().expect("nf: BuildAbs missing body");
                done.push(DBExpr::abs(name, body_nf));
            }
            Step::BuildNeutral { head_level, k, depth } => {
                let mut args: Vec<DBExpr> = Vec::with_capacity(k);
                for _ in 0..k {
                    args.push(done.pop().expect("nf: BuildNeutral missing arg"));
                }
                args.reverse();
                let mut result = DBExpr::Var(depth - 1 - head_level);
                for a in args {
                    result = DBExpr::app(result, a);
                }
                done.push(result);
            }
        }
    }

    debug_assert_eq!(done.len(), 1, "nf: expected one result, got {}", done.len());
    Ok(done.pop().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dvar(i: usize) -> DBExpr {
        DBExpr::Var(i)
    }

    #[test]
    fn lookup_returns_pushed_thunk_at_index_zero() {
        let env: Env = empty_env();
        let t = Thunk::pending(dvar(0), empty_env());
        let env = extend(&env, t.clone());
        // Index 0 should be the thunk we just pushed.
        assert!(Rc::ptr_eq(&lookup(&env, 0), &t));
    }

    #[test]
    fn lookup_resolves_outer_via_higher_index() {
        let env: Env = empty_env();
        let outer = Thunk::pending(dvar(7), empty_env());
        let inner = Thunk::pending(dvar(8), empty_env());
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

    fn budget() -> Budget {
        Budget::new(10_000)
    }

    #[test]
    fn whnf_of_lambda_is_self() {
        let term = dabs("x", dvar(0));
        let c = assert_closure(whnf(&term, &empty_env(), &mut budget()).unwrap());
        assert_eq!(c.body, dvar(0));
        assert_eq!(c.binder_name, "x");
        assert!(c.env.is_none());
    }

    #[test]
    fn whnf_of_identity_app() {
        let id = dabs("x", dvar(0));
        let id2 = dabs("y", dvar(0));
        let term = dapp(id, id2);
        let c = assert_closure(whnf(&term, &empty_env(), &mut budget()).unwrap());
        assert_eq!(c.body, dvar(0));
        assert_eq!(c.binder_name, "y");
    }

    #[test]
    fn whnf_const_picks_first() {
        let const_fn = dabs("x", dabs("y", dvar(1)));
        let arg1 = dabs("a", dvar(0));
        let arg2 = dabs("b", dvar(0));
        let term = dapp(dapp(const_fn, arg1), arg2);
        let c = assert_closure(whnf(&term, &empty_env(), &mut budget()).unwrap());
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
        let c = assert_closure(whnf(&term, &empty_env(), &mut budget()).unwrap());
        assert_eq!(c.binder_name, "z");
        assert_eq!(c.body, dvar(0));
    }

    #[test]
    fn whnf_omega_exhausts_budget() {
        // (\x. x x) (\x. x x) — Ω. Should hit the step limit.
        let omega_lambda = dabs("x", dapp(dvar(0), dvar(0)));
        let term = dapp(omega_lambda.clone(), omega_lambda);
        let mut b = Budget::new(100);
        assert!(matches!(
            whnf(&term, &empty_env(), &mut b),
            Err(EvalError::StepLimitExceeded(100)),
        ));
    }

    // ---- nf ----

    #[test]
    fn nf_identity() {
        // (\x. x) (\y. y)  →  \y. y
        let id = dabs("x", dvar(0));
        let id2 = dabs("y", dvar(0));
        let term = dapp(id, id2);
        let result = nf(&term, &empty_env(), 0, &mut budget()).unwrap();
        assert_eq!(result, dabs("y", dvar(0)));
    }

    #[test]
    fn nf_under_lambda_descends() {
        // \f. (\y. y) f  →  \f. f
        let inner = dapp(dabs("y", dvar(0)), dvar(0));
        let term = dabs("f", inner);
        let result = nf(&term, &empty_env(), 0, &mut budget()).unwrap();
        assert_eq!(result, dabs("f", dvar(0)));
    }

    #[test]
    fn nf_church_two_normalizes_to_self() {
        let two = dabs(
            "f",
            dabs("x", dapp(dvar(1), dapp(dvar(1), dvar(0)))),
        );
        let result = nf(&two, &empty_env(), 0, &mut budget()).unwrap();
        assert_eq!(result, two);
    }

    #[test]
    fn forced_thunk_can_be_replaced() {
        // Smoke-test the mutability story: take a Pending cell, replace
        // its contents with Forced — all references see the change.
        let cell = Thunk::pending(dvar(0), empty_env());
        let observer = Rc::clone(&cell);

        let dummy_closure = Closure {
            body: dvar(0),
            env: empty_env(),
            binder_name: "x".into(),
        };
        *cell.borrow_mut() = Thunk::Forced(dummy_closure);

        let is_forced = matches!(&*observer.borrow(), Thunk::Forced(_));
        assert!(is_forced, "observer still sees Pending after replacement");
    }
}
