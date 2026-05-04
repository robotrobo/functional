//! Runtime driver for `IO` actions. Walks an `IOAction` tree built by
//! WHNF and performs the actual side effects via injectable `IoSource`
//! and `IoSink` traits.

use std::cell::RefCell;
use std::rc::Rc;

use crate::cbn::{whnf, Budget, Env, EnvNode, IOAction, Thunk, Value};
use crate::debruijn::DBExpr;
use crate::error::EvalError;

/// Source of input for `readNat`.
pub trait IoSource {
    fn read_line(&self) -> Option<String>;
}

/// Output sink for `print`.
pub trait IoSink {
    fn writeln_nat(&self, n: u64);
}

/// Production stdin source.
pub struct StdinSource;

impl IoSource for StdinSource {
    fn read_line(&self) -> Option<String> {
        let mut buf = String::new();
        match std::io::stdin().read_line(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(buf.trim_end_matches('\n').trim_end_matches('\r').to_string()),
            Err(_) => None,
        }
    }
}

/// Production stdout sink.
pub struct StdoutSink;

impl IoSink for StdoutSink {
    fn writeln_nat(&self, n: u64) {
        println!("{}", n);
    }
}

fn force_to_nat(
    term: &DBExpr,
    env: &Env,
    budget: &mut Budget,
) -> Result<u64, EvalError> {
    let v = whnf(term, env, budget)?;
    match v {
        Value::Nat(n) => Ok(n),
        other => Err(EvalError::Runtime(format!(
            "expected Nat, got {:?}",
            other
        ))),
    }
}

/// Apply a continuation `(k_term, k_env) : a -> IO b` to a runtime `Value`,
/// producing the next `IOAction` to execute.
///
/// For self-contained args (`Nat`, `Unit`) we build `App(k_term, arg_db)` and
/// call `whnf` directly — this handles both lambda-closures and
/// partially-saturated primitives (e.g. bare `print`) in one path.
///
/// For heap-allocated args (`Cls`, `IOAction`) we first reduce `k_term` to a
/// closure, then manually beta-reduce by binding a forced thunk in the
/// closure's env. (De Bruijn index shifting would be needed to fold them into
/// the `App` trick, so we avoid it.)
fn apply_continuation(
    k_term: &DBExpr,
    k_env: &Env,
    arg: Value,
    budget: &mut Budget,
) -> Result<Rc<IOAction>, EvalError> {
    let result_val = match arg {
        Value::Nat(n) => {
            let app = DBExpr::app(k_term.clone(), DBExpr::NatLit(n));
            whnf(&app, k_env, budget)?
        }
        Value::Unit => {
            let app = DBExpr::app(k_term.clone(), DBExpr::UnitLit);
            whnf(&app, k_env, budget)?
        }
        Value::Cls(c) => {
            let closure = match whnf(k_term, k_env, budget)? {
                Value::Cls(cl) => cl,
                other => return Err(EvalError::Runtime(format!(
                    "continuation is not a function: {:?}", other
                ))),
            };
            let arg_node = Rc::new(EnvNode {
                thunk: RefCell::new(Thunk::Forced(c)),
                tail: closure.env.clone(),
            });
            whnf(&closure.body, &Some(arg_node), budget)?
        }
        Value::IOAction(a) => {
            let closure = match whnf(k_term, k_env, budget)? {
                Value::Cls(cl) => cl,
                other => return Err(EvalError::Runtime(format!(
                    "continuation is not a function: {:?}", other
                ))),
            };
            let arg_node = Rc::new(EnvNode {
                thunk: RefCell::new(Thunk::ForcedIOAction(a)),
                tail: closure.env.clone(),
            });
            whnf(&closure.body, &Some(arg_node), budget)?
        }
        Value::Neu { .. } | Value::StuckApp { .. } => {
            return Err(EvalError::Runtime(
                "continuation arg is a stuck/neutral value".into(),
            ));
        }
    };

    match result_val {
        Value::IOAction(a) => Ok(a),
        other => Err(EvalError::Runtime(format!(
            "continuation body is not an IO action: {:?}",
            other
        ))),
    }
}

/// Drive an `IOAction` tree, performing side effects.
///
/// `source` is borrowed (`&dyn IoSource`) because each `read_line` call is
/// stateful but doesn't outlive the driver. `sink` is owned (`Rc<dyn IoSink>`)
/// because the recursive `Bind` call clones it to share ownership across the
/// inner recursion — there's no shorter-lived borrow path that satisfies
/// the recursive ownership pattern.
pub fn run_io(
    action: &Rc<IOAction>,
    source: &dyn IoSource,
    sink: Rc<dyn IoSink>,
    budget: &mut Budget,
) -> Result<Value, EvalError> {
    let mut current: Rc<IOAction> = Rc::clone(action);
    loop {
        // Take a borrow for matching, then drop it before reassigning current.
        let next: Option<Rc<IOAction>> = match &*current {
            IOAction::Pure(t, e) => return whnf(t, e, budget),
            IOAction::Print(t, e) => {
                let n = force_to_nat(t, e, budget)?;
                sink.writeln_nat(n);
                return Ok(Value::Unit);
            }
            IOAction::ReadNat => {
                let line = source.read_line().ok_or_else(|| {
                    EvalError::Runtime("readNat: end of input".into())
                })?;
                let n: u64 = line.trim().parse().map_err(|_| {
                    EvalError::Runtime(format!(
                        "readNat: could not parse '{}' as Nat",
                        line.trim()
                    ))
                })?;
                return Ok(Value::Nat(n));
            }
            IOAction::Bind(inner, k_t, k_e) => {
                let inner_val = run_io(inner, source, Rc::clone(&sink), budget)?;
                let next_action = apply_continuation(k_t, k_e, inner_val, budget)?;
                Some(next_action)
            }
        };
        if let Some(n) = next {
            current = n;
        }
    }
}
