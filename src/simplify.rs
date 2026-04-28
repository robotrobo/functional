use crate::ast::Expr;
use crate::eval::{alpha_eq, free_vars, subst};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Occ {
    Zero,
    OneSafe,
    Many,
}

impl Occ {
    fn join(self, other: Occ) -> Occ {
        match (self, other) {
            (Occ::Zero, o) | (o, Occ::Zero) => o,
            _ => Occ::Many,
        }
    }
}

fn occurs(e: &Expr, x: &str) -> Occ {
    match e {
        Expr::Var(v) => {
            if v == x {
                Occ::OneSafe
            } else {
                Occ::Zero
            }
        }
        Expr::Abs(param, body) => {
            if param == x {
                Occ::Zero
            } else {
                match occurs(body, x) {
                    Occ::Zero => Occ::Zero,
                    _ => Occ::Many,
                }
            }
        }
        Expr::App(f, a) => occurs(f, x).join(occurs(a, x)),
    }
}

fn step(e: &Expr) -> Expr {
    match e {
        Expr::Var(_) => e.clone(),
        Expr::Abs(param, body) => {
            let body = step(body);
            if let Expr::App(f, arg) = &body {
                if let Expr::Var(v) = &**arg {
                    if v == param && !free_vars(f).contains(param) {
                        return (**f).clone();
                    }
                }
            }
            Expr::abs(param, body)
        }
        Expr::App(f, a) => {
            let f = step(f);
            let a = step(a);
            if let Expr::Abs(param, body) = &f {
                match occurs(body, param) {
                    Occ::Zero => return (**body).clone(),
                    Occ::OneSafe => return subst(body, param, &a),
                    Occ::Many => {
                        if matches!(a, Expr::Var(_)) {
                            return subst(body, param, &a);
                        }
                    }
                }
            }
            Expr::app(f, a)
        }
    }
}

pub fn simplify(e: &Expr) -> Expr {
    const MAX_ITERS: usize = 1000;
    let mut current = e.clone();
    for _ in 0..MAX_ITERS {
        let next = step(&current);
        if alpha_eq(&current, &next) {
            return next;
        }
        current = next;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: &str) -> Expr {
        Expr::var(n)
    }
    fn lam(p: &str, b: Expr) -> Expr {
        Expr::abs(p, b)
    }
    fn app(f: Expr, a: Expr) -> Expr {
        Expr::app(f, a)
    }

    #[test]
    fn occurs_counts_zero_one_many() {
        assert_eq!(occurs(&v("y"), "x"), Occ::Zero);
        assert_eq!(occurs(&v("x"), "x"), Occ::OneSafe);
        assert_eq!(occurs(&app(v("x"), v("x")), "x"), Occ::Many);
    }

    #[test]
    fn occurs_under_lambda_is_many() {
        // single occurrence under λ → Many (blocks linear inlining)
        let e = lam("y", v("x"));
        assert_eq!(occurs(&e, "x"), Occ::Many);
    }

    #[test]
    fn occurs_shadowed_param_is_zero() {
        // x shadowed by λx, body uses inner x — outer x has zero occurrences
        let e = lam("x", v("x"));
        assert_eq!(occurs(&e, "x"), Occ::Zero);
    }

    // -------- Rule 1: η-reduction --------

    #[test]
    fn eta_reduces_when_param_not_free_in_f() {
        // \x. f x → f
        let e = lam("x", app(v("f"), v("x")));
        assert_eq!(simplify(&e), v("f"));
    }

    #[test]
    fn eta_blocked_when_param_free_in_f() {
        // \x. (g x) x — param x is free in (g x); η must not fire
        let e = lam("x", app(app(v("g"), v("x")), v("x")));
        // After simplify nothing should change (no other rule applies either).
        assert_eq!(simplify(&e), e);
    }

    // -------- Rule 2: Dead-arg drop --------

    #[test]
    fn dead_arg_dropped() {
        // (\x. y) N → y
        let e = app(lam("x", v("y")), v("anything"));
        assert_eq!(simplify(&e), v("y"));
    }

    #[test]
    fn dead_arg_drops_complex_arg() {
        // (\x. y) (some app) → y; the arg is silently discarded
        let arg = app(v("a"), v("b"));
        let e = app(lam("x", v("y")), arg);
        assert_eq!(simplify(&e), v("y"));
    }

    // -------- Rule 3: Var inline --------

    #[test]
    fn var_arg_is_inlined_even_when_used_many_times() {
        // (\x. x x) y → y y — N is a Var so duplication is free
        let e = app(lam("x", app(v("x"), v("x"))), v("y"));
        assert_eq!(simplify(&e), app(v("y"), v("y")));
    }

    // -------- Rule 4: Linear inline --------

    #[test]
    fn linear_use_inlined() {
        // (\x. f x) (g a) → f (g a) — x used once, not under λ
        let arg = app(v("g"), v("a"));
        let e = app(lam("x", app(v("f"), v("x"))), arg.clone());
        assert_eq!(simplify(&e), app(v("f"), arg));
    }

    #[test]
    fn linear_inline_blocked_under_lambda() {
        // (\x. \y. x) heavy must NOT inline — x occurs once but under λy.
        // This is the canonical work-duplication regression test.
        let heavy = app(v("h1"), v("h2"));
        let e = app(lam("x", lam("y", v("x"))), heavy.clone());
        // Allowed result: leave the App in place.
        assert_eq!(simplify(&e), app(lam("x", lam("y", v("x"))), heavy));
    }

    #[test]
    fn linear_inline_blocked_when_used_twice() {
        // (\x. f x x) (g a) — x occurs twice, must not inline (would duplicate work).
        let arg = app(v("g"), v("a"));
        let e = app(lam("x", app(app(v("f"), v("x")), v("x"))), arg.clone());
        assert_eq!(
            simplify(&e),
            app(lam("x", app(app(v("f"), v("x")), v("x"))), arg)
        );
    }

    // -------- Fixpoint iteration --------

    #[test]
    fn iterates_to_fixpoint() {
        // (\x. \y. y x) f a — first inline f for x, then a for y.
        // Step 1: \y. y f, applied to a → linear inline a for y → f a... wait y appears once but
        // in `y f` y is at top of the body — not under any λ. So inline → a f.
        // Actually: (\x. \y. y x) f → linear-inline f for x → \y. y f.
        // Then \y. y f applied to a → linear-inline a for y → a f.
        let e = app(app(lam("x", lam("y", app(v("y"), v("x")))), v("f")), v("a"));
        assert_eq!(simplify(&e), app(v("a"), v("f")));
    }
}
