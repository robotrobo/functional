//! End-to-end tests for primitive Nat operations: parse + typecheck +
//! evaluate, asserting both the inferred type and the runtime value.

use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::infer::infer_program;
use lc::parser::parse_program;
use lc::pretty::print;
use lc::simplify::simplify;
use lc::types::Type;

fn run(src: &str) -> (Option<Type>, String) {
    let p = parse_program(src).expect("parse");
    let types = infer_program(&p);
    let main_t = types.main_type.and_then(|r| r.ok());
    let inlined = inline_defs(&p).expect("inline");
    let simplified = simplify(&inlined);
    let nf = normalize(&simplified, 1_000_000).expect("normalize");
    (main_t, print(&nf))
}

fn run_with_prelude(src: &str) -> (Option<Type>, String) {
    let prelude = std::fs::read_to_string("lib/prelude.lc").expect("read prelude");
    let prelude_p = parse_program(&prelude).expect("parse prelude");
    let user_p = parse_program(src).expect("parse user");
    let mut defs = prelude_p.defs;
    defs.extend(user_p.defs);
    let program = Program {
        defs,
        main: user_p.main,
    };
    let types = infer_program(&program);
    let main_t = types.main_type.and_then(|r| r.ok());
    let inlined = inline_defs(&program).expect("inline");
    let simplified = simplify(&inlined);
    let nf = normalize(&simplified, 1_000_000).expect("normalize");
    (main_t, print(&nf))
}

#[test]
fn add_two_literals() {
    let (t, s) = run("add 2 3");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "5");
}

#[test]
fn mul_with_pred() {
    let (t, s) = run("mul (pred 5) 2");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "8");
}

#[test]
fn ifz_zero_branch() {
    let (t, s) = run("ifz 0 100 200");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "100");
}

#[test]
fn ifz_nonzero_branch() {
    let (t, s) = run("ifz 7 100 200");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "200");
}

#[test]
fn factorial_of_five() {
    // Uses the `fact` def from prelude.lc
    let (_t, s) = run_with_prelude("fact 5");
    assert_eq!(s, "120");
}

#[test]
fn factorial_of_seven() {
    let (_t, s) = run_with_prelude("fact 7");
    assert_eq!(s, "5040");
}

#[test]
fn sub_saturates() {
    let (t, s) = run("sub 3 10");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "0");
}

#[test]
fn fact_typechecks_to_nat_to_nat() {
    // Type-check only; don't normalize — `fact` as a bare value has body
    // that contains a primitive applied to a bound variable, which the
    // CBN evaluator can't reduce under a binder without an actual Nat.
    let prelude = std::fs::read_to_string("lib/prelude.lc").expect("read prelude");
    let prelude_p = parse_program(&prelude).expect("parse prelude");
    let user_p = parse_program("fact").expect("parse user");
    let mut defs = prelude_p.defs;
    defs.extend(user_p.defs);
    let program = Program {
        defs,
        main: user_p.main,
    };
    let types = infer_program(&program);
    let main_t = types.main_type.unwrap().unwrap();
    assert_eq!(main_t, Type::arrow(Type::Nat, Type::Nat));
}

#[test]
fn ifz_else_branch_with_nat_subtraction() {
    // sub 10 3 = 7 (nonzero), so ifz takes the else branch = 0.
    let (t, s) = run("ifz (sub 10 3) (succ 0) 0");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "0");
}

#[test]
fn ifz_then_branch_when_condition_is_zero() {
    // sub 3 3 = 0, so ifz takes the then branch = succ 0 = 1.
    let (t, s) = run("ifz (sub 3 3) (succ 0) 99");
    assert_eq!(t, Some(Type::Nat));
    assert_eq!(s, "1");
}
