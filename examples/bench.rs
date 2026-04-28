//! Tiny ad-hoc bench: compare named substitution vs De Bruijn on a
//! non-trivial Church-numeral computation.
//!
//! Run with: `cargo run --release --example bench`

use std::time::Instant;

use lc::ast::Expr;
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

fn run_case(label: &str, user_src: &str, limit: usize) {
    let prelude = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let combined = format!("{}\n{}", prelude, user_src);
    let prog = parse_program(&combined).unwrap();
    let inlined = inline_defs(&prog).unwrap();

    let t0 = Instant::now();
    let r_named = named_normalize(&inlined, limit);
    let t_named = t0.elapsed();

    let t0 = Instant::now();
    let r_db = db_normalize(&inlined, limit);
    let t_db = t0.elapsed();

    let speedup = t_named.as_secs_f64() / t_db.as_secs_f64();
    let same = match (&r_named, &r_db) {
        (Ok(a), Ok(b)) => alpha_eq(a, b),
        _ => false,
    };

    println!(
        "{label:<24}  named: {t_named:>10.3?}  db: {t_db:>10.3?}  speedup: {speedup:>5.2}x  agree: {same}"
    );
}

fn main() {
    println!("== lc bench (named vs De Bruijn) ==\n");
    run_case("fact 3", "fact three", 1_000_000);
    run_case("fact 4", "fact four", 5_000_000);
    run_case("fact 5", "fact five", 20_000_000);
    run_case("pow 2 4", "pow two four", 1_000_000);
    run_case("add 3 4", "add three four", 1_000);
}
