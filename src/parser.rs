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
    ident().map(Expr::Var)
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
}
