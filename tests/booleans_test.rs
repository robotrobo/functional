use lc::ast::Expr;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;

const STEP_LIMIT: usize = 1000;

/// Run the full pipeline (read file → parse → inline defs → normalize).
fn evaluate(path: &str) -> Expr {
    let src = std::fs::read_to_string(path).expect("read source file");
    let program = parse_program(&src).expect("parse should succeed");
    let inlined = inline_defs(&program).expect("inline_defs should succeed");
    normalize(&inlined, STEP_LIMIT).expect("normalize should succeed")
}

/// Church-encoded `true` as a structural expectation: `\a. \b. a`.
fn church_true() -> Expr {
    Expr::abs("a", Expr::abs("b", Expr::var("a")))
}

/// Church-encoded `false` as a structural expectation: `\a. \b. b`.
fn church_false() -> Expr {
    Expr::abs("a", Expr::abs("b", Expr::var("b")))
}

#[test]
fn and_true_false_is_false() {
    assert_eq!(evaluate("examples/booleans/and_tf.lc"), church_false());
}

#[test]
fn and_true_true_is_true() {
    assert_eq!(evaluate("examples/booleans/and_tt.lc"), church_true());
}

#[test]
fn or_true_false_is_true() {
    assert_eq!(evaluate("examples/booleans/or_ft.lc"), church_true());
}

#[test]
fn not_true_is_false() {
    assert_eq!(evaluate("examples/booleans/not_t.lc"), church_false());
}
