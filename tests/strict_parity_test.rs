//! Parity tests for M6.3 strictness analysis. For each program, run the
//! full pipeline with strict on and off; assert results α-equivalent and
//! that strict-on never consumes more CBN steps than strict-off.

use lc::ast::Program;
use lc::eval::{alpha_eq, inline_defs, normalize_with_options};
use lc::parser::parse_program;
use lc::simplify::simplify;

const STEP_LIMIT: usize = 10_000_000;

fn run(user_src: &str, strict: bool) -> (lc::ast::Expr, usize) {
    let prelude_src = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let prelude = parse_program(&prelude_src).unwrap();
    let user = parse_program(user_src).unwrap();
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    let program = Program {
        defs,
        main: user.main,
    };
    let inlined = inline_defs(&program).unwrap();
    let prepared = simplify(&inlined);
    normalize_with_options(&prepared, STEP_LIMIT, strict).unwrap()
}

fn check(src: &str) {
    let (off_nf, off_steps) = run(src, false);
    let (on_nf, on_steps) = run(src, true);
    assert!(
        alpha_eq(&off_nf, &on_nf),
        "strictness changed semantics on: {src}",
    );
    assert!(
        on_steps <= off_steps,
        "strictness regressed step count on `{src}`: off={off_steps}, on={on_steps}",
    );
}

#[test]
fn parity_add() {
    check("add 1 2");
}

#[test]
fn parity_mul() {
    check("mul 2 3");
}

#[test]
fn parity_fact_three() {
    check("fact 3");
}

#[test]
fn parity_pred_three() {
    check("pred 3");
}

#[test]
fn parity_list_length() {
    check("length (cons 1 (cons 2 (cons 3 nil)))");
}

#[test]
fn parity_map_succ() {
    check("length (map succ (cons 1 (cons 2 nil)))");
}

#[test]
fn parity_compose() {
    check("compose succ succ 1");
}

#[test]
fn parity_if_true() {
    check("if true 1 2");
}

#[test]
fn parity_if_false() {
    check("if false 1 2");
}
