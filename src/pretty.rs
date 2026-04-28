use crate::ast::Expr;

pub fn print(e: &Expr) -> String {
    if let Some(pretty) = detect_church_shape(e) {
        return pretty;
    }
    print_expr(e)
}

/// Try to recognize a Church-encoded value (numeral, boolean, or list) and
/// produce a friendly rendering. Returns `None` if `e` is not a recognizable
/// Church shape; the caller should fall back to raw lambda printing.
fn detect_church_shape(e: &Expr) -> Option<String> {
    if let Some(n) = as_church_numeral(e) {
        return Some(n.to_string());
    }
    if let Some(b) = as_church_bool(e) {
        return Some(b.to_string());
    }
    if let Some(items) = as_church_list(e) {
        let inner = items
            .iter()
            .map(|item| print(item))
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!("[{}]", inner));
    }
    None
}

/// If `e` is `\f. \x. f^n x` (for any binder names), return `n`.
/// The detection must be α-aware: the names `f` and `x` are not literal —
/// they're whatever the two binders happen to be called.
fn as_church_numeral(_e: &Expr) -> Option<usize> {
    // TODO: implement
    match _e {
        Expr::Abs(f, body) => match &**body {
            Expr::Abs(x, body2) => {
                if f == x {
                    return None;
                };
                count_apps(body2, f, x)
            }
            _ => None,
        },
        _ => None,
    }
}

fn count_apps(e: &Expr, f: &str, x: &str) -> Option<usize> {
    let mut count = 0;

    let mut peeled = e;

    loop {
        match peeled {
            Expr::Var(_x) if _x == x => {
                return Some(count);
            }

            Expr::App(v, inner) => match &**v {
                Expr::Var(_f) if _f == f => {
                    peeled = inner;
                    count += 1;
                    continue;
                }
                _ => return None,
            },

            _ => return None,
        }
    }
}

/// If `e` is `\t. \f. t` return Some(true). If `\t. \f. f` return Some(false).
/// Again, binder names are arbitrary — the test is structural.
fn as_church_bool(_e: &Expr) -> Option<bool> {
    // TODO: implement
    let Expr::Abs(t, body) = _e else {
        return None;
    };

    let Expr::Abs(f, body2) = &**body else {
        return None;
    };

    if let Expr::Var(_t) = &**body2 {
        if _t == t {
            return Some(true);
        }
        if _t == f {
            return Some(false);
        }

        return None;
    };

    None
}

/// If `e` is a Church-encoded list `\c. \n. c h0 (c h1 (... (c hk n)))`,
/// return the list of head expressions. Returns None for anything else,
/// including the empty list `\c. \n. n` (it's ambiguous with other shapes —
/// you may choose to handle it or not).
fn as_church_list(e: &Expr) -> Option<Vec<Expr>> {
    let Expr::Abs(c, body1) = e else {
        return None;
    };
    let Expr::Abs(n, body2) = &**body1 else {
        return None;
    };
    if c == n {
        return None;
    }

    let mut items: Vec<Expr> = Vec::new();
    let mut peeled: &Expr = body2;
    loop {
        match peeled {
            // base case: hit the nil binder → done
            Expr::Var(name) if name == n => return Some(items),
            // cons case: App(App(Var c, head), rest)
            Expr::App(outer, rest) => {
                let Expr::App(c_var, head) = &**outer else {
                    return None;
                };
                let Expr::Var(name) = &**c_var else {
                    return None;
                };
                if name != c {
                    return None;
                }
                items.push((**head).clone());
                peeled = rest;
            }
            _ => return None,
        }
    }
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
        let e = Expr::app(Expr::app(Expr::var("f"), Expr::var("x")), Expr::var("y"));
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
            Expr::app(Expr::app(Expr::var("f"), Expr::var("x")), Expr::var("y")),
        );
        assert_eq!(print(&e), "\\x. f x y");
    }

    #[test]
    fn print_nested_application_lhs_parens() {
        // (\x. x) y — lambda on the left of an application must be parenthesized
        let e = Expr::app(Expr::abs("x", Expr::var("x")), Expr::var("y"));
        assert_eq!(print(&e), "(\\x. x) y");
    }
    #[test]
    fn print_church_list_of_numerals() {
        // \c. \n. c (\f. \x. f x) (c (\f. \x. f (f x)) n)  →  "[1, 2]"
        let one = Expr::abs(
            "f",
            Expr::abs("x", Expr::app(Expr::var("f"), Expr::var("x"))),
        );
        let two = Expr::abs(
            "f",
            Expr::abs(
                "x",
                Expr::app(Expr::var("f"), Expr::app(Expr::var("f"), Expr::var("x"))),
            ),
        );
        let body = Expr::app(
            Expr::app(Expr::var("c"), one),
            Expr::app(Expr::app(Expr::var("c"), two), Expr::var("n")),
        );
        let list = Expr::abs("c", Expr::abs("n", body));
        assert_eq!(print(&list), "[1, 2]");
    }
}
