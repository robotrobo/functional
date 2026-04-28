//! Tiny ad-hoc bench: compare named substitution, De Bruijn substitution,
//! and call-by-need (current `normalize`) on Church-numeral programs.
//!
//! Run with: `cargo run --release --example bench`

use std::time::Instant;

use lc::ast::Expr;
use lc::cbn;
use lc::debruijn;
use lc::error::EvalError;
use lc::eval::{alpha_eq, inline_defs, reduce_step};
use lc::parser::parse_program;

fn named_normalize(e: &Expr, max_steps: usize) -> Result<Expr, EvalError> {
    let mut current = e.clone();
    for _ in 0..max_steps {
        match reduce_step(&current) {
            Some(next) => current = next,
            None => return Ok(current),
        }
    }
    Err(EvalError::StepLimitExceeded(max_steps))
}

fn db_normalize(e: &Expr, max_steps: usize) -> Result<Expr, EvalError> {
    let mut current = debruijn::to_db(e);
    for _ in 0..max_steps {
        match debruijn::reduce_step(&current) {
            Some(next) => current = next,
            None => return Ok(debruijn::to_named(&current)),
        }
    }
    Err(EvalError::StepLimitExceeded(max_steps))
}

fn cbn_normalize(e: &Expr) -> Expr {
    let db = debruijn::to_db(e);
    let mut budget = cbn::Budget::new(10_000_000);
    let result = cbn::nf(&db, &cbn::empty_env(), 0, &mut budget).unwrap();
    debruijn::to_named(&result)
}

fn run_case(label: &str, user_src: &str, named_limit: usize) {
    let prelude = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let combined = format!("{}\n{}", prelude, user_src);
    let prog = parse_program(&combined).unwrap();
    let inlined = inline_defs(&prog).unwrap();

    let t0 = Instant::now();
    let r_named = named_normalize(&inlined, named_limit);
    let t_named = t0.elapsed();

    let t0 = Instant::now();
    let r_db = db_normalize(&inlined, named_limit);
    let t_db = t0.elapsed();

    let t0 = Instant::now();
    let r_cbn = cbn_normalize(&inlined);
    let t_cbn = t0.elapsed();

    let agree = match (&r_named, &r_db) {
        (Ok(a), Ok(b)) => alpha_eq(a, b) && alpha_eq(a, &r_cbn),
        _ => false,
    };

    println!(
        "{label:<14}  named: {t_named:>10.3?}  db-subst: {t_db:>10.3?}  cbn: {t_cbn:>10.3?}  agree: {agree}"
    );
}

fn main() {
    println!("== lc bench (named subst / DB subst / call-by-need) ==\n");
    run_case("fact 3", "fact three", 1_000_000);
    run_case("fact 4", "fact four", 5_000_000);
    run_case("fact 5", "fact five", 20_000_000);
    run_case("pow 2 4", "pow two four", 1_000_000);
    run_case("add 3 4", "add three four", 1_000);

    // Harder: skip the named/db comparisons (they'd take forever) — just CBN.
    let prelude = std::fs::read_to_string("lib/prelude.lc").unwrap();
    for (label, src) in [
        ("fact 6", "fact (succ five)"),
        ("fact 7", "fact (succ (succ five))"),
        ("mul 5 5", "mul five five"),
        ("pow 3 3", "pow three three"),
    ] {
        let combined = format!("{}\n{}", prelude, src);
        let prog = parse_program(&combined).unwrap();
        let inlined = inline_defs(&prog).unwrap();
        let t0 = Instant::now();
        let _ = cbn_normalize(&inlined);
        let t = t0.elapsed();
        println!("{label:<14}  cbn-only: {t:>10.3?}");
    }
}
