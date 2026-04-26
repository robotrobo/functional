use chumsky::prelude::*;

use crate::ast::Expr;
use crate::error::ParseError;

fn ident() -> impl Parser<char, String, Error = Simple<char>> {
    filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .map(|c| c.to_string())
        .chain::<char, _, _>(
            filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_').repeated(),
        )
        .collect::<String>()
        .padded()
}

fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr| {
        let var = ident().map(Expr::Var);

        let lambda = just('\\')
            .ignore_then(ident())
            .then_ignore(just('.').padded())
            .then(expr.clone())
            .map(|(param, body)| Expr::abs(param, body));

        let parens = expr
            .clone()
            .delimited_by(just('(').padded(), just(')').padded());

        let atom = choice((lambda, parens, var)).padded();

        atom.repeated()
            .at_least(1)
            .map(|atoms| {
                let mut iter = atoms.into_iter();
                let head = iter.next().unwrap();
                iter.fold(head, Expr::app)
            })
    })
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    expr_parser()
        .then_ignore(end())
        .parse(src)
        .map_err(|errs| {
            ParseError::Generic(
                errs.into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_identifier() {
        assert_eq!(parse_expr("x").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_identifier_with_whitespace() {
        assert_eq!(parse_expr("  x  ").unwrap(), Expr::var("x"));
    }

    #[test]
    fn parse_underscore_identifier() {
        assert_eq!(parse_expr("_foo").unwrap(), Expr::var("_foo"));
    }

    #[test]
    fn parse_lambda_identity() {
        assert_eq!(
            parse_expr("\\x. x").unwrap(),
            Expr::abs("x", Expr::var("x"))
        );
    }

    #[test]
    fn parse_application_left_assoc() {
        assert_eq!(
            parse_expr("f x y").unwrap(),
            Expr::app(
                Expr::app(Expr::var("f"), Expr::var("x")),
                Expr::var("y"),
            )
        );
    }

    #[test]
    fn parse_lambda_body_extends_right() {
        assert_eq!(
            parse_expr("\\x. f x").unwrap(),
            Expr::abs("x", Expr::app(Expr::var("f"), Expr::var("x")))
        );
    }

    #[test]
    fn parse_parenthesized_application() {
        assert_eq!(
            parse_expr("(\\x. x) y").unwrap(),
            Expr::app(
                Expr::abs("x", Expr::var("x")),
                Expr::var("y"),
            )
        );
    }

    #[test]
    fn parse_y_combinator() {
        let inner = Expr::abs(
            "x",
            Expr::app(
                Expr::var("f"),
                Expr::app(Expr::var("x"), Expr::var("x")),
            ),
        );
        let expected = Expr::abs("f", Expr::app(inner.clone(), inner));
        assert_eq!(
            parse_expr("\\f. (\\x. f (x x)) (\\x. f (x x))").unwrap(),
            expected
        );
    }
}
