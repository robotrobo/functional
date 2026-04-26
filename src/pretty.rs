use crate::ast::Expr;

pub fn print(e: &Expr) -> String {
    print_expr(e)
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
        let e = Expr::app(
            Expr::app(Expr::var("f"), Expr::var("x")),
            Expr::var("y"),
        );
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
            Expr::app(
                Expr::app(Expr::var("f"), Expr::var("x")),
                Expr::var("y"),
            ),
        );
        assert_eq!(print(&e), "\\x. f x y");
    }

    #[test]
    fn print_nested_application_lhs_parens() {
        // (\x. x) y — lambda on the left of an application must be parenthesized
        let e = Expr::app(Expr::abs("x", Expr::var("x")), Expr::var("y"));
        assert_eq!(print(&e), "(\\x. x) y");
    }
}
