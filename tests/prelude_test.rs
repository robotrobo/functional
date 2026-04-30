//! Tests for every operator defined in lib/prelude.lc.
//!
//! Each test prepends the prelude to a small main expression, runs it
//! through the full pipeline (parse → inline_defs → normalize), and
//! asserts the normal form matches the expected value.
//!
//! After the Nat migration: numeric values are primitive `Expr::NatLit`,
//! arithmetic is via the `succ`/`pred`/`add`/`sub`/`mul`/`ifz` keywords.
//! Booleans, pairs, and lists remain Church-encoded.

use lc::ast::Expr;
use lc::eval::{inline_defs, normalize};
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
    assert_eq!(evaluate("id 0"), Expr::nat(0));
}

#[test]
fn const_returns_first_argument() {
    assert_eq!(evaluate("const 0 1"), Expr::nat(0));
}

#[test]
fn flip_swaps_first_two_arguments() {
    // flip sub 2 5  =  sub 5 2  =  3
    assert_eq!(evaluate("flip sub 2 5"), Expr::nat(3));
}

#[test]
fn compose_chains_two_functions() {
    // compose succ succ 0  =  succ (succ 0)  =  2
    assert_eq!(evaluate("compose succ succ 0"), Expr::nat(2));
}

// ---- Tier 2: booleans (still Church) ----

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
    assert_eq!(evaluate("if true 0 1"), Expr::nat(0));
}

#[test]
fn if_false_picks_else_branch() {
    assert_eq!(evaluate("if false 0 1"), Expr::nat(1));
}

// ---- Tier 3: primitive Nat operators ----

#[test]
fn nat_zero_literal() {
    assert_eq!(evaluate("0"), Expr::nat(0));
}

#[test]
fn succ_of_zero_is_one() {
    assert_eq!(evaluate("succ 0"), Expr::nat(1));
}

#[test]
fn three_literal() {
    assert_eq!(evaluate("3"), Expr::nat(3));
}

#[test]
fn add_two_three_is_five() {
    assert_eq!(evaluate("add 2 3"), Expr::nat(5));
}

#[test]
fn mul_two_three_is_six() {
    assert_eq!(evaluate("mul 2 3"), Expr::nat(6));
}

#[test]
fn ifz_zero_is_then_branch() {
    assert_eq!(evaluate("ifz 0 100 200"), Expr::nat(100));
}

#[test]
fn ifz_nonzero_is_else_branch() {
    assert_eq!(evaluate("ifz 5 100 200"), Expr::nat(200));
}

// ---- Tier 4: pairs (still Church) ----

#[test]
fn fst_of_pair() {
    assert_eq!(evaluate("fst (pair 0 1)"), Expr::nat(0));
}

#[test]
fn snd_of_pair() {
    assert_eq!(evaluate("snd (pair 0 1)"), Expr::nat(1));
}

// ---- Tier 5: pred / sub ----

#[test]
fn pred_of_three_is_two() {
    assert_eq!(evaluate("pred 3"), Expr::nat(2));
}

#[test]
fn pred_of_zero_is_zero() {
    assert_eq!(evaluate("pred 0"), Expr::nat(0));
}

#[test]
fn sub_five_two_is_three() {
    assert_eq!(evaluate("sub 5 2"), Expr::nat(3));
}

// ---- Tier 6: recursion via fix ----

#[test]
fn fact_zero_is_one() {
    assert_eq!(evaluate("fact 0"), Expr::nat(1));
}

#[test]
fn fact_one_is_one() {
    assert_eq!(evaluate("fact 1"), Expr::nat(1));
}

#[test]
fn fact_two_is_two() {
    assert_eq!(evaluate("fact 2"), Expr::nat(2));
}

#[test]
fn fact_five_is_120() {
    assert_eq!(evaluate("fact 5"), Expr::nat(120));
}

// ---- Tier 7: lists (still Church-encoded) ----

#[test]
fn is_nil_of_nil_is_true() {
    assert_eq!(evaluate("isNil nil"), church_true());
}

#[test]
fn is_nil_of_cons_is_false() {
    assert_eq!(evaluate("isNil (cons 0 nil)"), church_false());
}

#[test]
fn length_of_nil_is_zero() {
    assert_eq!(evaluate("length nil"), Expr::nat(0));
}

#[test]
fn length_of_two_element_list_is_two() {
    assert_eq!(
        evaluate("length (cons 1 (cons 2 nil))"),
        Expr::nat(2),
    );
}

#[test]
fn foldr_sums_a_list() {
    // foldr add 0 [1, 2, 3]  =  6
    assert_eq!(
        evaluate("foldr add 0 (cons 1 (cons 2 (cons 3 nil)))"),
        Expr::nat(6),
    );
}

#[test]
fn map_succ_increments_each() {
    // map succ [1, 2]  =  [2, 3]
    assert_eq!(
        evaluate("map succ (cons 1 (cons 2 nil))"),
        church_list(vec![Expr::nat(2), Expr::nat(3)]),
    );
}

#[test]
fn append_concatenates_lists() {
    // append [1] [2]  =  [1, 2]
    assert_eq!(
        evaluate("append (cons 1 nil) (cons 2 nil)"),
        church_list(vec![Expr::nat(1), Expr::nat(2)]),
    );
}
