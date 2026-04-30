use lc::ast::Expr;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;

const STEP_LIMIT: usize = 1_000_000;

fn evaluate(path: &str) -> Expr {
    let src = std::fs::read_to_string(path).expect("read source file");
    let program = parse_program(&src).expect("parse should succeed");
    let inlined = inline_defs(&program).expect("inline_defs should succeed");
    normalize(&inlined, STEP_LIMIT).expect("normalize should succeed")
}

#[test]
fn factorial_of_three_is_six() {
    assert_eq!(
        evaluate("examples/factorial/fact_3.lc"),
        Expr::nat(6),
    );
}
