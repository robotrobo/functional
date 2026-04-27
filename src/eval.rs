use std::collections::HashSet;

use crate::{
    ast::{Def, Expr, Program},
    error::EvalError,
};

fn fresh_name(name: &str, taken: &HashSet<String>) -> String {
    let mut candidate = format!("{name}'");
    while taken.contains(&candidate) {
        candidate = format!("{candidate}'")
    }
    candidate
}

pub fn subst(target: &Expr, x: &str, value: &Expr) -> Expr {
    match target {
        Expr::Var(v) => {
            if v == x {
                value.clone()
            } else {
                target.clone()
            }
        }
        Expr::Abs(param, body) => {
            if param == x {
                // shadowing the outer param
                target.clone()
            } else {
                let mut taken = free_vars(value);
                let (param, body) = if taken.contains(param) {
                    taken.extend(free_vars(body));
                    taken.insert(x.to_string());
                    let new_param = fresh_name(param, &taken);
                    (
                        new_param.clone(),
                        subst(body, param, &Expr::var(&new_param)),
                    )
                } else {
                    (param.clone(), (**body).clone())
                };
                Expr::abs(param, subst(&body, x, value))
            }
        }
        Expr::App(e1, e2) => Expr::app(subst(e1, x, value), subst(e2, x, value)),
    }
}

pub fn reduce_step(e: &Expr) -> Option<Expr> {
    match e {
        Expr::App(f, a) => {
            if let Expr::Abs(param, body) = &**f {
                Some(subst(body, param, a))
            } else if let Some(f2) = reduce_step(f) {
                Some(Expr::app(f2, (**a).clone()))
            } else {
                reduce_step(a).map(|a2| Expr::app((**f).clone(), a2))
            }
        }
        Expr::Abs(p, body) => reduce_step(body).map(|f| Expr::abs(p, f)),
        _ => None,
    }
}

pub fn normalize(e: &Expr, max_steps: usize) -> Result<Expr, EvalError> {
    let mut current = e.clone();
    for _ in 0..max_steps {
        match reduce_step(&current) {
            Some(next) => current = next,
            None => return Ok(current),
        }
    }
    Err(EvalError::StepLimitExceeded(max_steps))
}

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
        resolved.push(Def {
            name: d.name.clone(),
            body,
        });
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

pub fn free_vars(e: &Expr) -> HashSet<String> {
    match e {
        Expr::Var(name) => HashSet::from([name.clone()]),
        Expr::Abs(param, body) => {
            let mut f = free_vars(body);
            f.remove(param);
            f
        }
        Expr::App(f, a) => {
            let mut set = free_vars(f);
            set.extend(free_vars(a));
            set
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    #[test]
    fn substitution_avoids_capture() {
        // subst(\y. x, x, y) should produce \y'. y, not \y. y
        let target = Expr::abs("y", Expr::var("x"));
        let result = subst(&target, "x", &Expr::var("y"));
        assert_eq!(result, Expr::abs("y'", Expr::var("y")));
    }

    #[test]
    fn substitution_into_variable() {
        assert_eq!(subst(&Expr::var("x"), "x", &Expr::var("y")), Expr::var("y"));
        assert_eq!(subst(&Expr::var("z"), "x", &Expr::var("y")), Expr::var("z"));
    }

    #[test]
    fn substitution_skips_shadowed_lambda() {
        // subst(\x. x, x, y) should be unchanged — inner x shadows
        let target = Expr::abs("x", Expr::var("x"));
        let result = subst(&target, "x", &Expr::var("y"));
        assert_eq!(result, target);
    }

    #[test]
    fn identity_applied_to_identity() {
        // (\x. x) (\y. y)  →  \y. y
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let stepped = reduce_step(&e).unwrap();
        assert_eq!(stepped, Expr::abs("y", Expr::var("y")));
    }

    #[test]
    fn no_redex_returns_none() {
        let e = Expr::abs("x", Expr::var("x"));
        assert!(reduce_step(&e).is_none());
    }
    use std::collections::BTreeSet;

    fn fv_set(e: &Expr) -> BTreeSet<String> {
        super::free_vars(e).into_iter().collect()
    }

    #[test]
    fn free_var_of_variable() {
        assert_eq!(
            fv_set(&Expr::var("x")),
            ["x"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn free_var_of_lambda_excludes_param() {
        assert_eq!(fv_set(&Expr::abs("x", Expr::var("x"))), BTreeSet::new());
        let yfree = Expr::abs("x", Expr::var("y"));
        assert_eq!(
            fv_set(&yfree),
            ["y"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn free_var_of_app() {
        let e = Expr::app(Expr::var("f"), Expr::var("x"));
        assert_eq!(
            fv_set(&e),
            ["f", "x"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn step_avoids_capture() {
        // (\x. \y. x) y  →  \y'. y   (NOT \y. y)
        let e = Expr::app(
            Expr::abs("x", Expr::abs("y", Expr::var("x"))),
            Expr::var("y"),
        );
        let stepped = reduce_step(&e).unwrap();
        assert_eq!(stepped, Expr::abs("y'", Expr::var("y")));
    }

    #[test]
    fn step_under_lambda() {
        // \x. (\y. y) x  →  \x. x
        let inner = Expr::app(Expr::abs("y", Expr::var("y")), Expr::var("x"));
        let e = Expr::abs("x", inner);
        let stepped = reduce_step(&e).unwrap();
        assert_eq!(stepped, Expr::abs("x", Expr::var("x")));
    }

    //    - Identity: (\x. x) (\y. y) → \y. y
    #[test]
    fn normalize_id() {
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let stepped = normalize(&e, 100).unwrap();
        assert_eq!(stepped, Expr::abs("y", Expr::var("y")));
    }

    // - Multi-step: (\x. \y. x) a b → a (two β-steps)
    #[test]
    fn normalize_multi_step() {
        let e = Expr::app(
            Expr::app(
                Expr::abs("x", Expr::abs("y", Expr::var("x"))),
                Expr::var("a"),
            ),
            Expr::var("b"),
        );
        let stepped = normalize(&e, 100).unwrap();
        assert_eq!(stepped, Expr::var("a"));
    }
    // - Step limit hit: (\x. x x) (\x. x x) should return Err(StepLimitExceeded(...))
    #[test]
    fn normalize_step_limit_omega() {
        let e = Expr::app(
            Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x"))),
            Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x"))),
        );
        assert!(matches!(
            normalize(&e, 10000),
            Err(EvalError::StepLimitExceeded(10000))
        ));
    }

    use crate::ast::{Def, Program};

    #[test]
    fn inline_defs_into_main() {
        // def id = \x. x ; main = id (\z. z)  →  (\x. x) (\z. z)
        // (Plan spec used `id y`, but `y` would be free in the result; using
        //  a closed argument keeps the test consistent with the closed-term
        //  check inside inline_defs.)
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: Some(Expr::app(
                Expr::var("id"),
                Expr::abs("z", Expr::var("z")),
            )),
        };
        let inlined = inline_defs(&p).unwrap();
        assert_eq!(
            inlined,
            Expr::app(
                Expr::abs("x", Expr::var("x")),
                Expr::abs("z", Expr::var("z")),
            )
        );
    }

    #[test]
    fn inline_chained_defs() {
        // def a = \x. x ; def b = a ; main = b  →  \x. x
        let p = Program {
            defs: vec![
                Def {
                    name: "a".into(),
                    body: Expr::abs("x", Expr::var("x")),
                },
                Def {
                    name: "b".into(),
                    body: Expr::var("a"),
                },
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
}
