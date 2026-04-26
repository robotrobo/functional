use chumsky::prelude::*;

use crate::ast::Expr;
use crate::ast::{Def, Program};
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

/// Strip `-- ...` to end-of-line comments. Operates line by line so byte
/// offsets shift uniformly per line; close enough for our purposes.
fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn def_parser() -> impl Parser<char, Def, Error = Simple<char>> {
    text::keyword("def")
        .padded()
        .ignore_then(ident())
        .then_ignore(just('=').padded())
        .then(expr_parser())
        .map(|(name, body)| Def { name, body })
}

pub fn parse_program(src: &str) -> Result<Program, ParseError> {
    let cleaned = strip_comments(src);

    // Line-based: each non-empty line is either a `def ...` or the main
    // expression. The main expression (if present) must be the last
    // non-empty line. Multi-line expressions are not supported in v1; use
    // parens to keep things on one line if needed.
    let lines: Vec<(usize, &str)> = cleaned
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .collect();

    let mut defs = Vec::new();
    let mut main = None;

    for (i, (lineno, line)) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let is_last = i == lines.len() - 1;

        if trimmed.starts_with("def") && trimmed.len() > 3
            && !trimmed.as_bytes()[3].is_ascii_alphanumeric()
            && trimmed.as_bytes()[3] != b'_'
        {
            // Parse this line as a def.
            let parsed = def_parser()
                .then_ignore(end())
                .parse(*line)
                .map_err(|errs| {
                    ParseError::Generic(format!(
                        "line {}: {}",
                        lineno + 1,
                        errs.into_iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ))
                })?;
            defs.push(parsed);
        } else {
            // Must be a main expression and must be the last non-empty line.
            if !is_last {
                return Err(ParseError::Generic(format!(
                    "line {}: expression appears before end of file (must be last non-empty line)",
                    lineno + 1
                )));
            }
            let parsed = expr_parser()
                .then_ignore(end())
                .parse(*line)
                .map_err(|errs| {
                    ParseError::Generic(format!(
                        "line {}: {}",
                        lineno + 1,
                        errs.into_iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ))
                })?;
            main = Some(parsed);
        }
    }

    Ok(Program { defs, main })
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

    use crate::ast::{Def, Program};

    fn parse_program_test(src: &str) -> Result<Program, ParseError> {
        super::parse_program(src)
    }

    #[test]
    fn parse_empty_program() {
        let p = parse_program_test("").unwrap();
        assert!(p.defs.is_empty());
        assert!(p.main.is_none());
    }

    #[test]
    fn parse_program_with_one_def_and_main() {
        let p = parse_program_test("def id = \\x. x\nid x").unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.defs[0].name, "id");
        assert_eq!(p.defs[0].body, Expr::abs("x", Expr::var("x")));
        assert_eq!(
            p.main.as_ref().unwrap(),
            &Expr::app(Expr::var("id"), Expr::var("x"))
        );
    }

    #[test]
    fn parse_program_with_comments() {
        let src = "-- this is a comment\n\
                   def id = \\x. x  -- inline comment\n\
                   id";
        let p = parse_program_test(src).unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.main.as_ref().unwrap(), &Expr::var("id"));
    }
}
