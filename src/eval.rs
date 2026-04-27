use std::collections::HashSet;

use crate::ast::Expr;

/// **NAIVE** substitution. Will be replaced in Task 11 with capture-avoiding
/// substitution. Correct only on terms where no capture is possible.
pub fn naive_subst(target: &Expr, x: &str, value: &Expr) -> Expr {
    match target {
        Expr::Var(v) => {
            if v.eq(x) {
                value.clone()
            } else {
                target.clone()
            }
        }
        Expr::Abs(param, body) => {
            if param.eq(x) {
                // shadowing the outer param
                target.clone()
            } else {
                Expr::abs(param.clone(), naive_subst(body, x, value))
            }
        }
        Expr::App(e1, e2) => Expr::app(naive_subst(e1, x, value), naive_subst(e2, x, value)),
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
            } else if let Some(a2) = naive_reduce_step(a) {
                Some(Expr::app((**f).clone(), a2))
            } else {
                None
            }
        }
        _ => None,
    }
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
}
