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
    assert_eq!(run_with_prelude("add 1 2"), "3");
}

#[test]
fn fact_three_via_prelude() {
    assert_eq!(run_with_prelude("fact 3"), "6");
}

#[test]
fn list_length_three() {
    assert_eq!(
        run_with_prelude("length (cons 1 (cons 2 (cons 3 nil)))"),
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

// `three` (named Church-encoded numeral) was removed when the prelude
// migrated to primitive Nat. The literal `3` now stands alone.

// -------- let bindings (parse-time desugar to App-Abs) --------

#[test]
fn let_binds_a_value() {
    assert_eq!(run_with_prelude("let x = 5 in x"), "5");
}

#[test]
fn let_uses_binding_in_expression() {
    assert_eq!(run_with_prelude("let x = 3 in add x x"), "6");
}

#[test]
fn nested_let_shares_results() {
    // let x = mul 6 7 in let y = succ x in y  →  43
    assert_eq!(
        run_with_prelude("let x = mul 6 7 in let y = succ x in y"),
        "43",
    );
}

#[test]
fn let_inside_lambda_body() {
    // (\n. let sq = mul n n in add sq sq) 3  →  18
    assert_eq!(
        run_with_prelude("(\\n. let sq = mul n n in add sq sq) 3"),
        "18",
    );
}
