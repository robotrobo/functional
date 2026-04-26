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
