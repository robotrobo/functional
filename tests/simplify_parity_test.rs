//! For each program of interest, run the full pipeline with the simplifier
//! on and off; assert results are α-equivalent and that simplify-on never
//! consumes more CBN steps than simplify-off.

use lc::ast::Program;
use lc::eval::{alpha_eq, inline_defs, normalize_with_steps};
use lc::parser::parse_program;
use lc::simplify::simplify;

const STEP_LIMIT: usize = 10_000_000;

fn pipeline(user_src: &str, do_simplify: bool) -> (lc::ast::Expr, usize) {
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
    let prepared = if do_simplify { simplify(&inlined) } else { inlined };
    normalize_with_steps(&prepared, STEP_LIMIT).unwrap()
}

fn check_parity(src: &str) {
    let (off_nf, off_steps) = pipeline(src, false);
    let (on_nf, on_steps) = pipeline(src, true);
    assert!(
        alpha_eq(&off_nf, &on_nf),
        "simplify changed semantics on: {src}",
    );
    assert!(
        on_steps <= off_steps,
        "simplify regressed step count on `{src}`: off={off_steps}, on={on_steps}",
    );
}

#[test]
fn parity_add_one_two() {
    check_parity("add 1 2");
}

#[test]
fn parity_fact_three() {
    check_parity("fact 3");
}

#[test]
fn parity_list_length() {
    check_parity("length (cons 1 (cons 2 (cons 3 nil)))");
}

#[test]
fn parity_mul_two_three() {
    check_parity("mul 2 3");
}

#[test]
fn parity_pred_three() {
    check_parity("pred 3");
}

#[test]
fn parity_map_succ() {
    check_parity("length (map succ (cons 1 (cons 2 nil)))");
}

#[test]
fn parity_compose_pipeline() {
    check_parity("compose succ succ 1");
}
