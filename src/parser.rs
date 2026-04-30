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
            if matches!(
                s.as_str(),
                "def" | "let" | "in" | "fix" | "succ" | "pred" | "add" | "sub" | "mul" | "ifz"
            ) {
                Err(Simple::custom(span, format!("unexpected keyword `{s}`")))
            } else {
                Ok(s)
            }
        })
        .then_ignore(hws())
}

/// Build the Church numeral for `n`: `\f. \x. f^n x`. Used to elaborate
/// numeric literals at parse time. The result is a closed term that does
/// not depend on `succ`/`zero` being defined — so numeric literals work
/// even without the prelude loaded.
fn church_numeral(n: u64) -> Expr {
    let mut body = Expr::var("x");
    for _ in 0..n {
        body = Expr::app(Expr::var("f"), body);
    }
    Expr::abs("f", Expr::abs("x", body))
}

fn expr_parser() -> impl Parser<char, Expr, Error = Simple<char>> {
    recursive(|expr| {
        let var = ident().map(Expr::Var);

        // Decimal literal → Church numeral. Parses one or more digits; a
        // bare digit run is not a valid identifier (idents must start with
        // a letter or _), so this never collides with `var`.
        let numeral = filter(|c: &char| c.is_ascii_digit())
            .repeated()
            .at_least(1)
            .collect::<String>()
            .then_ignore(hws())
            .try_map(|s: String, span| {
                s.parse::<u64>()
                    .map(church_numeral)
                    .map_err(|e| Simple::custom(span, format!("invalid numeric literal: {e}")))
            });

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

        // let x = e1 in e2  →  (\x. e2) e1
        // Non-recursive: x is NOT in scope inside e1, only inside e2.
        // Sequential let-chains fall out for free since e2 is itself an expr.
        let let_in = text::keyword("let")
            .then_ignore(hws())
            .ignore_then(ident())
            .then_ignore(just('='))
            .then_ignore(hws())
            .then(expr.clone())
            .then_ignore(text::keyword("in"))
            .then_ignore(hws())
            .then(expr.clone())
            .map(|((name, e1), e2)| Expr::app(Expr::abs(name, e2), e1));

        // `fix <atom>` — binds tighter than juxtaposition, so `fix f x`
        // parses as `(fix f) x`. The argument is a single atom (paren'd
        // expr, lambda, numeral, var, or another `fix`).
        let fix_atom = recursive(|fix_atom| {
            let inner_lambda = just('\\')
                .then_ignore(hws())
                .ignore_then(ident())
                .then_ignore(just('.'))
                .then_ignore(hws())
                .then(expr.clone())
                .map(|(p, b)| Expr::abs(p, b));
            let inner_parens = just('(')
                .then_ignore(hws())
                .ignore_then(expr.clone())
                .then_ignore(just(')'))
                .then_ignore(hws());
            let inner_var = ident().map(Expr::Var);

            text::keyword("fix")
                .then_ignore(hws())
                .ignore_then(choice((inner_parens, inner_lambda, fix_atom, inner_var)))
                .map(Expr::fix)
        });

        // Each primitive name parses as Expr::Prim(...). They're reserved
        // by `ident()`, so they cannot be shadowed by lambda binders or
        // `def`s.
        let prim_atom = choice((
            text::keyword("succ").to(Expr::prim(crate::ast::PrimOp::Succ)),
            text::keyword("pred").to(Expr::prim(crate::ast::PrimOp::Pred)),
            text::keyword("add").to(Expr::prim(crate::ast::PrimOp::Add)),
            text::keyword("sub").to(Expr::prim(crate::ast::PrimOp::Sub)),
            text::keyword("mul").to(Expr::prim(crate::ast::PrimOp::Mul)),
            text::keyword("ifz").to(Expr::prim(crate::ast::PrimOp::IfZ)),
        ))
        .then_ignore(hws());

        let atom = choice((fix_atom, let_in, lambda, parens, prim_atom, numeral, var));

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

/// Convert a 0-based char-position in `src` to a (line, col) pair, both 1-indexed.
fn byte_to_line_col(src: &str, byte: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in src.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Return the 1-indexed `line`'s contents (no trailing newline).
fn line_at(src: &str, line: usize) -> &str {
    src.lines().nth(line.saturating_sub(1)).unwrap_or("")
}

fn fmt_token(opt: Option<&char>) -> String {
    match opt {
        Some(c) => format!("'{}'", c),
        None => "end of input".to_string(),
    }
}

/// Render a single chumsky error against the original source. Output shape
/// is modeled on Rust compiler errors: header, location arrow, source line,
/// caret.
fn render_one(err: &Simple<char>, src: &str) -> String {
    let span = err.span();
    let (line, col) = byte_to_line_col(src, span.start);
    let found = fmt_token(err.found());

    let mut expected: Vec<String> = err.expected().map(|o| fmt_token(o.as_ref())).collect();
    expected.sort();
    expected.dedup();

    let expected_str = match expected.len() {
        0 => "something else".to_string(),
        1 => expected.remove(0),
        _ => {
            let last = expected.pop().unwrap();
            format!("{}, or {}", expected.join(", "), last)
        }
    };

    let header = format!("error: expected {}, found {}", expected_str, found);
    let line_num = line.to_string();
    let pad = " ".repeat(line_num.len());
    let location = format!("{}--> {}:{}", " ".repeat(line_num.len() + 1), line, col);
    let separator = format!(" {} |", pad);
    let source = format!(" {} | {}", line_num, line_at(src, line));
    let caret = format!(" {} | {}^", pad, " ".repeat(col.saturating_sub(1)));

    format!("{header}\n{location}\n{separator}\n{source}\n{caret}")
}

fn render_errors(errs: Vec<Simple<char>>, src: &str) -> ParseError {
    let message = errs
        .iter()
        .map(|e| render_one(e, src))
        .collect::<Vec<_>>()
        .join("\n\n");
    ParseError::new(message)
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let cleaned = strip_comments(src);
    let normalized = collapse_newlines_in_parens(&cleaned);
    hws()
        .ignore_then(expr_parser())
        .then_ignore(end())
        .parse(normalized.as_str())
        .map_err(|errs| render_errors(errs, &normalized))
}

pub fn parse_program(src: &str) -> Result<Program, ParseError> {
    let cleaned = strip_comments(src);
    let normalized = collapse_newlines_in_parens(&cleaned);
    program_parser()
        .parse(normalized.as_str())
        .map_err(|errs| render_errors(errs, &normalized))
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
    fn parse_zero_literal() {
        // 0 → \f. \x. x
        assert_eq!(
            parse_expr("0").unwrap(),
            Expr::abs("f", Expr::abs("x", Expr::var("x"))),
        );
    }

    #[test]
    fn parse_one_literal() {
        // 1 → \f. \x. f x
        assert_eq!(
            parse_expr("1").unwrap(),
            Expr::abs(
                "f",
                Expr::abs("x", Expr::app(Expr::var("f"), Expr::var("x"))),
            ),
        );
    }

    #[test]
    fn parse_three_literal() {
        // 3 → \f. \x. f (f (f x))
        let inner = Expr::app(
            Expr::var("f"),
            Expr::app(Expr::var("f"), Expr::app(Expr::var("f"), Expr::var("x"))),
        );
        assert_eq!(
            parse_expr("3").unwrap(),
            Expr::abs("f", Expr::abs("x", inner)),
        );
    }

    #[test]
    fn parse_numeric_literal_in_application() {
        // `add 1 2` — primitive `add` applied to two numeric literals.
        // The literal shape (still Church until T8) is checked via the
        // recursive parse so this stays correct across T8.
        assert_eq!(
            parse_expr("add 1 2").unwrap(),
            Expr::app(
                Expr::app(Expr::prim(crate::ast::PrimOp::Add), parse_expr("1").unwrap()),
                parse_expr("2").unwrap(),
            ),
        );
    }

    #[test]
    fn parse_let_in_basic() {
        // let x = a in x  →  (\x. x) a
        assert_eq!(
            parse_expr("let x = a in x").unwrap(),
            Expr::app(Expr::abs("x", Expr::var("x")), Expr::var("a")),
        );
    }

    #[test]
    fn parse_nested_let() {
        // let x = a in let y = b in x  →  (\x. (\y. x) b) a
        let inner = Expr::app(Expr::abs("y", Expr::var("x")), Expr::var("b"));
        let expected = Expr::app(Expr::abs("x", inner), Expr::var("a"));
        assert_eq!(parse_expr("let x = a in let y = b in x").unwrap(), expected);
    }

    #[test]
    fn parse_let_binding_can_use_application() {
        // let x = f y in x  →  (\x. x) (f y)
        assert_eq!(
            parse_expr("let x = f y in x").unwrap(),
            Expr::app(
                Expr::abs("x", Expr::var("x")),
                Expr::app(Expr::var("f"), Expr::var("y")),
            ),
        );
    }

    #[test]
    fn parse_let_inside_lambda_body() {
        // \x. let y = x in y  →  \x. (\y. y) x
        let body = Expr::app(Expr::abs("y", Expr::var("y")), Expr::var("x"));
        assert_eq!(parse_expr("\\x. let y = x in y").unwrap(), Expr::abs("x", body));
    }

    #[test]
    fn identifiers_starting_with_digit_still_rejected() {
        // `5x` should NOT parse as identifier (idents start with letter/_).
        // `5 x` parses as `5` applied to `x`. This test confirms the
        // numeric and identifier alphabets stay disjoint at the start.
        assert!(parse_expr("5abc").is_err() || {
            // Acceptable shape: parsed as `5` applied to `abc` after a
            // tokenization gap. Either failure or correct app is fine —
            // what we want to forbid is a single ident named "5abc".
            let parsed = parse_expr("5abc").unwrap();
            matches!(parsed, Expr::App(..))
        });
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
    fn parse_fix_simple() {
        // fix (\x. x)
        assert_eq!(
            parse_expr("fix (\\x. x)").unwrap(),
            Expr::fix(Expr::abs("x", Expr::var("x"))),
        );
    }

    #[test]
    fn parse_fix_in_application() {
        // `fix f x` — fix binds tighter than juxtaposition.
        assert_eq!(
            parse_expr("fix f x").unwrap(),
            Expr::app(Expr::fix(Expr::var("f")), Expr::var("x")),
        );
    }

    #[test]
    fn fix_is_reserved_identifier() {
        // \fix. fix should fail — fix is reserved.
        assert!(parse_expr("\\fix. fix").is_err());
    }

    #[test]
    fn parse_succ_keyword() {
        assert_eq!(
            parse_expr("succ").unwrap(),
            Expr::prim(crate::ast::PrimOp::Succ),
        );
    }

    #[test]
    fn parse_add_application_with_vars() {
        // add x y — primitive applied to two variables.
        assert_eq!(
            parse_expr("add x y").unwrap(),
            Expr::app(
                Expr::app(Expr::prim(crate::ast::PrimOp::Add), Expr::var("x")),
                Expr::var("y"),
            ),
        );
    }

    #[test]
    fn primitive_name_cannot_be_a_binder() {
        assert!(parse_expr("\\add. add").is_err());
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
