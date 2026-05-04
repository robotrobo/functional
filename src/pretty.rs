use crate::ast::Expr;

pub fn print(e: &Expr) -> String {
    if let Some(pretty) = detect_church_shape(e) {
        return pretty;
    }
    print_expr(e)
}

/// Try to recognize a Church-encoded list and produce a friendly rendering.
/// Numerals and booleans are NOT detected — Church-shaped lambdas like
/// `\f. \x. x` are ambiguous (they could be Church 0, Church false, or
/// just a polymorphic function `\_. id`). Under HM typing, the inferred
/// type tells the user what the value really is; the printer should not
/// guess one interpretation. List shape is distinctive enough to keep.
fn detect_church_shape(e: &Expr) -> Option<String> {
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
            // base case: hit the nil binder → done. Only count this as a
            // list if we actually saw at least one element — `\c. \n. n`
            // alone is too ambiguous (could be Church zero, false, or a
            // generic K-ish function).
            Expr::Var(name) if name == n => {
                if items.is_empty() {
                    return None;
                }
                return Some(items);
            }
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
                Expr::App(_, _) | Expr::Abs(_, _) | Expr::Fix(_) => {
                    format!("({})", print_expr(x))
                }
                _ => print_expr(x),
            };
            format!("{} {}", f_str, x_str)
        }
        Expr::Fix(inner) => {
            let inner_str = match **inner {
                Expr::Abs(_, _) | Expr::App(_, _) | Expr::Fix(_) => {
                    format!("({})", print_expr(inner))
                }
                _ => print_expr(inner),
            };
            format!("fix {}", inner_str)
        }
        Expr::NatLit(n) => n.to_string(),
        Expr::UnitLit => "()".to_string(),
        Expr::Prim(op) => op.name().to_string(),
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
    fn print_church_list_of_nat_lits() {
        // \c. \n. c 1 (c 2 n)  →  "[1, 2]"
        let body = Expr::app(
            Expr::app(Expr::var("c"), Expr::nat(1)),
            Expr::app(Expr::app(Expr::var("c"), Expr::nat(2)), Expr::var("n")),
        );
        let list = Expr::abs("c", Expr::abs("n", body));
        assert_eq!(print(&list), "[1, 2]");
    }

    #[test]
    fn church_numeral_shape_no_longer_printed_as_digit() {
        // After Nat migration, `\f. \x. x` is just a function — not "0".
        let e = Expr::abs("f", Expr::abs("x", Expr::var("x")));
        assert_eq!(print(&e), "\\f. \\x. x");
    }

    #[test]
    fn print_unit_literal() {
        assert_eq!(print(&Expr::UnitLit), "()");
    }
}
