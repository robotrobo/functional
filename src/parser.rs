use chumsky::prelude::*;

use crate::ast::Expr;
use crate::ast::{Def, Program};
use crate::error::ParseError;

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

/// Collapse newlines that appear *inside* matched parens to spaces.
/// This is run after `strip_comments` so the source seen by chumsky has
/// horizontal-whitespace-only inside parens, while top-level newlines are
/// preserved as item separators. Tracks paren depth via a single counter.
fn collapse_newlines_in_parens(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut depth: i32 = 0;
    for c in src.chars() {
        match c {
            '(' => {
                depth += 1;
                out.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                out.push(c);
            }
            '\n' if depth > 0 => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Horizontal whitespace: spaces, tabs, carriage returns. Excludes `\n`.
/// Used inside expressions where newlines must terminate.
fn hws() -> impl Parser<char, (), Error = Simple<char>> + Clone + Copy {
    filter(|c: &char| *c == ' ' || *c == '\t' || *c == '\r')
        .repeated()
        .ignored()
}

/// One or more newlines, with horizontal whitespace allowed before/after.
/// Used as the separator between top-level items.
fn item_sep() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    hws()
        .then(filter(|c: &char| *c == '\n'))
        .then(hws().then(filter(|c: &char| *c == '\n')).repeated())
        .then(hws())
        .ignored()
}

/// Identifier, with `def` reserved.
fn ident() -> impl Parser<char, String, Error = Simple<char>> + Clone {
    filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .map(|c| c.to_string())
        .chain::<char, _, _>(
            filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_').repeated(),
        )
        .collect::<String>()
        .try_map(|s, span| {
            if s == "def" {
                Err(Simple::custom(span, "unexpected keyword `def`"))
            } else {
                Ok(s)
            }
        })
        .then_ignore(hws())
}

fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr| {
        let var = ident().map(Expr::Var);

        let lambda = just('\\')
            .then_ignore(hws())
            .ignore_then(ident())
            .then_ignore(just('.'))
            .then_ignore(hws())
            .then(expr.clone())
            .map(|(p, b)| Expr::abs(p, b));

        let parens = just('(')
            .then_ignore(hws())
            .ignore_then(expr.clone())
            .then_ignore(just(')'))
            .then_ignore(hws());

        let atom = choice((lambda, parens, var));

        atom.repeated()
            .at_least(1)
            .map(|atoms| {
                let mut iter = atoms.into_iter();
                let head = iter.next().unwrap();
                iter.fold(head, Expr::app)
            })
    })
}

fn def_parser() -> impl Parser<char, Def, Error = Simple<char>> {
    text::keyword("def")
        .then_ignore(hws())
        .ignore_then(ident())
        .then_ignore(just('='))
        .then_ignore(hws())
        .then(expr_parser())
        .map(|(name, body)| Def { name, body })
}

fn program_parser() -> impl Parser<char, Program, Error = Simple<char>> {
    let leading = item_sep().or_not().ignored();
    let trailing = item_sep().or_not().ignored().then_ignore(end());

    leading
        .ignore_then(
            def_parser()
                .separated_by(item_sep())
                .allow_trailing(),
        )
        .then(expr_parser().or_not())
        .then_ignore(trailing)
        .map(|(defs, main)| Program { defs, main })
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let cleaned = strip_comments(src);
    let normalized = collapse_newlines_in_parens(&cleaned);
    hws()
        .ignore_then(expr_parser())
        .then_ignore(end())
        .parse(normalized.as_str())
        .map_err(|errs| {
            ParseError::Generic(
                errs.into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
}

pub fn parse_program(src: &str) -> Result<Program, ParseError> {
    let cleaned = strip_comments(src);
    let normalized = collapse_newlines_in_parens(&cleaned);
    program_parser().parse(normalized.as_str()).map_err(|errs| {
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

    #[test]
    fn parse_program_with_multiple_defs() {
        let src = "def a = \\x. x\ndef b = \\y. y\nb a";
        let p = super::parse_program(src).unwrap();
        assert_eq!(p.defs.len(), 2);
        assert_eq!(p.defs[0].name, "a");
        assert_eq!(p.defs[1].name, "b");
        assert_eq!(
            p.main.as_ref().unwrap(),
            &Expr::app(Expr::var("b"), Expr::var("a"))
        );
    }

    #[test]
    fn parse_program_rejects_def_as_identifier() {
        // `def` must be reserved
        let src = "def def = \\x. x";
        assert!(super::parse_program(src).is_err());
    }

    #[test]
    fn parse_program_with_multi_line_paren_expression() {
        // Newlines inside parens are folded to spaces by the preprocessor,
        // so a long expression can span lines if wrapped in parens.
        let src = "def big = (\\f.\n    \\x.\n    f (f x))\nbig";
        let p = super::parse_program(src).unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.defs[0].name, "big");
        // The body should reduce to the same AST as if written on one line.
        let expected = Expr::abs(
            "f",
            Expr::abs(
                "x",
                Expr::app(
                    Expr::var("f"),
                    Expr::app(Expr::var("f"), Expr::var("x")),
                ),
            ),
        );
        assert_eq!(p.defs[0].body, expected);
    }

    #[test]
    fn parse_program_with_blank_lines_between_defs() {
        let src = "def a = \\x. x\n\n\ndef b = \\y. y\n\nb a";
        let p = super::parse_program(src).unwrap();
        assert_eq!(p.defs.len(), 2);
        assert!(p.main.is_some());
    }
}
