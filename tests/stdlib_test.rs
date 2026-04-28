use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

fn run_with_prelude(user_src: &str) -> String {
    let prelude_src = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let prelude = parse_program(&prelude_src).unwrap();
    let user = parse_program(user_src).unwrap();
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    let program = Program { defs, main: user.main };
    let inlined = inline_defs(&program).unwrap();
    let nf = normalize(&inlined, 1_000_000).unwrap();
    print(&nf)
}

#[test]
fn add_one_two_is_three() {
    assert_eq!(run_with_prelude("add one two"), "3");
}

#[test]
fn fact_three_via_prelude() {
    assert_eq!(run_with_prelude("fact three"), "6");
}

#[test]
fn list_length_three() {
    assert_eq!(
        run_with_prelude("length (cons one (cons two (cons three nil)))"),
        "3",
    );
}

// -------- numeric literals (parse-time Church desugaring) --------

#[test]
fn literal_zero_prints_as_zero() {
    assert_eq!(run_with_prelude("0"), "0");
}

#[test]
fn literal_addition() {
    assert_eq!(run_with_prelude("add 5 3"), "8");
}

#[test]
fn literal_multiplication() {
    assert_eq!(run_with_prelude("mul 6 7"), "42");
}

#[test]
fn literal_factorial() {
    assert_eq!(run_with_prelude("fact 5"), "120");
}

#[test]
fn literal_and_named_numeral_agree() {
    // 3 (literal) should reduce to the same value as `three` (prelude def).
    assert_eq!(run_with_prelude("3"), run_with_prelude("three"));
}
