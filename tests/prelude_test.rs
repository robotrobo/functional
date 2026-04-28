//! Tests for every operator defined in lib/prelude.lc.
//!
//! Each test prepends the prelude to a small main expression, runs it
//! through the full pipeline (parse → inline_defs → normalize), and
//! asserts the normal form matches the expected Church-encoded value.
//!
//! Tests use the same binder names as the prelude (e.g. `\t. \f. t` for
//! true) — if you change a binder name in the prelude, the structural
//! equality check here will fail.

use lc::ast::Expr;
use lc::eval::{alpha_eq, inline_defs, normalize};
use lc::parser::parse_program;

const STEP_LIMIT: usize = 1_000_000;

fn evaluate(main_src: &str) -> Expr {
    let prelude = std::fs::read_to_string("lib/prelude.lc").expect("read lib/prelude.lc");
    let combined = format!("{}\n{}", prelude, main_src);
    let program = parse_program(&combined).expect("parse should succeed");
    let inlined = inline_defs(&program).expect("inline_defs should succeed");
    normalize(&inlined, STEP_LIMIT).expect("normalize should succeed")
}

// ---- expected-value builders ----

fn church_true() -> Expr {
    // \t. \f. t
    Expr::abs("t", Expr::abs("f", Expr::var("t")))
}

fn church_false() -> Expr {
    // \t. \f. f
    Expr::abs("t", Expr::abs("f", Expr::var("f")))
}

fn church_numeral(n: usize) -> Expr {
    // \f. \x. f^n x
    let mut body = Expr::var("x");
    for _ in 0..n {
        body = Expr::app(Expr::var("f"), body);
    }
    Expr::abs("f", Expr::abs("x", body))
}

fn church_list(items: Vec<Expr>) -> Expr {
    // \c. \n. c item0 (c item1 ... (c itemN n))
    let mut body = Expr::var("n");
    for item in items.into_iter().rev() {
        body = Expr::app(Expr::app(Expr::var("c"), item), body);
    }
    Expr::abs("c", Expr::abs("n", body))
}

// ---- Tier 1: combinators ----

#[test]
fn id_returns_its_argument() {
    assert_eq!(evaluate("id zero"), church_numeral(0));
}

#[test]
fn const_returns_first_argument() {
    assert_eq!(evaluate("const zero one"), church_numeral(0));
}

#[test]
fn flip_swaps_first_two_arguments() {
    // flip sub two five  =  sub five two  =  3
    assert_eq!(evaluate("flip sub two five"), church_numeral(3));
}

#[test]
fn compose_chains_two_functions() {
    // compose succ succ zero  =  succ (succ zero)  =  2
    assert_eq!(evaluate("compose succ succ zero"), church_numeral(2));
}

// ---- Tier 2: booleans ----

#[test]
fn true_is_first_selector() {
    assert_eq!(evaluate("true"), church_true());
}

#[test]
fn false_is_second_selector() {
    assert_eq!(evaluate("false"), church_false());
}

#[test]
fn not_of_true_is_false() {
    assert_eq!(evaluate("not true"), church_false());
}

#[test]
fn not_of_false_is_true() {
    assert_eq!(evaluate("not false"), church_true());
}

#[test]
fn and_true_true() {
    assert_eq!(evaluate("and true true"), church_true());
}

#[test]
fn and_true_false_is_false() {
    assert_eq!(evaluate("and true false"), church_false());
}

#[test]
fn and_false_true_is_false() {
    assert_eq!(evaluate("and false true"), church_false());
}

#[test]
fn or_true_false_is_true() {
    assert_eq!(evaluate("or true false"), church_true());
}

#[test]
fn or_false_false_is_false() {
    assert_eq!(evaluate("or false false"), church_false());
}

#[test]
fn if_true_picks_then_branch() {
    assert_eq!(evaluate("if true zero one"), church_numeral(0));
}

#[test]
fn if_false_picks_else_branch() {
    assert_eq!(evaluate("if false zero one"), church_numeral(1));
}

// ---- Tier 3a: numerals ----

#[test]
fn zero_is_church_zero() {
    assert_eq!(evaluate("zero"), church_numeral(0));
}

#[test]
fn succ_of_zero_is_one() {
    assert_eq!(evaluate("succ zero"), church_numeral(1));
}

#[test]
fn three_is_church_three() {
    assert_eq!(evaluate("three"), church_numeral(3));
}

#[test]
fn add_two_three_is_five() {
    assert_eq!(evaluate("add two three"), church_numeral(5));
}

#[test]
fn mul_two_three_is_six() {
    assert_eq!(evaluate("mul two three"), church_numeral(6));
}

#[test]
fn pow_two_three_is_eight() {
    // Result has α-renamed binders (\x. \x'. ...) but is α-equivalent to canonical Church 8.
    assert!(alpha_eq(&evaluate("pow two three"), &church_numeral(8)));
}

#[test]
fn is_zero_of_zero_is_true() {
    assert_eq!(evaluate("isZero zero"), church_true());
}

#[test]
fn is_zero_of_one_is_false() {
    assert_eq!(evaluate("isZero one"), church_false());
}

// ---- Tier 4: pairs ----

#[test]
fn fst_of_pair() {
    assert_eq!(evaluate("fst (pair zero one)"), church_numeral(0));
}

#[test]
fn snd_of_pair() {
    assert_eq!(evaluate("snd (pair zero one)"), church_numeral(1));
}

// ---- Tier 3b: pred / sub ----

#[test]
fn pred_of_three_is_two() {
    assert_eq!(evaluate("pred three"), church_numeral(2));
}

#[test]
fn pred_of_zero_is_zero() {
    assert_eq!(evaluate("pred zero"), church_numeral(0));
}

#[test]
fn sub_five_two_is_three() {
    assert_eq!(evaluate("sub five two"), church_numeral(3));
}

// ---- Tier 5: recursion ----

#[test]
fn fact_zero_is_one() {
    assert_eq!(evaluate("fact zero"), church_numeral(1));
}

#[test]
fn fact_one_is_one() {
    assert_eq!(evaluate("fact one"), church_numeral(1));
}

#[test]
fn fact_two_is_two() {
    assert_eq!(evaluate("fact two"), church_numeral(2));
}

// ---- Tier 6: lists ----

#[test]
fn is_nil_of_nil_is_true() {
    assert_eq!(evaluate("isNil nil"), church_true());
}

#[test]
fn is_nil_of_cons_is_false() {
    assert_eq!(evaluate("isNil (cons zero nil)"), church_false());
}

#[test]
fn length_of_nil_is_zero() {
    assert_eq!(evaluate("length nil"), church_numeral(0));
}

#[test]
fn length_of_two_element_list_is_two() {
    assert_eq!(
        evaluate("length (cons one (cons two nil))"),
        church_numeral(2),
    );
}

#[test]
fn foldr_sums_a_list() {
    // foldr add zero [1, 2, 3]  =  6
    assert_eq!(
        evaluate("foldr add zero (cons one (cons two (cons three nil)))"),
        church_numeral(6),
    );
}

#[test]
fn map_succ_increments_each() {
    // map succ [1, 2]  =  [2, 3]
    assert_eq!(
        evaluate("map succ (cons one (cons two nil))"),
        church_list(vec![church_numeral(2), church_numeral(3)]),
    );
}

#[test]
fn filter_keeps_matching_elements() {
    // filter isZero [0, 1, 0]  =  [0, 0]
    assert_eq!(
        evaluate("filter isZero (cons zero (cons one (cons zero nil)))"),
        church_list(vec![church_numeral(0), church_numeral(0)]),
    );
}

#[test]
fn append_concatenates_lists() {
    // append [1] [2]  =  [1, 2]
    assert_eq!(
        evaluate("append (cons one nil) (cons two nil)"),
        church_list(vec![church_numeral(1), church_numeral(2)]),
    );
}
