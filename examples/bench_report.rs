//! Quick measurement harness: runs each benchmark program in four
//! configurations (simplify on/off × strict on/off), prints CBN step
//! counts and wall-time. Not asserted — for human inspection.
//!
//! Usage: `cargo run --release --example bench_report`

use lc::ast::Program;
use lc::eval::{inline_defs, normalize_with_options};
use lc::parser::parse_program;
use lc::simplify::simplify;
use std::time::Instant;

const STEP_LIMIT: usize = 1_000_000_000;

fn pipeline(user_src: &str, do_simplify: bool, do_strict: bool) -> (usize, std::time::Duration) {
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
    let t = Instant::now();
    let (_, steps) = normalize_with_options(&prepared, STEP_LIMIT, do_strict).unwrap();
    (steps, t.elapsed())
}

fn pct(base: usize, new: usize) -> f64 {
    100.0 * (base as f64 - new as f64) / base as f64
}

fn report(label: &str, src: &str) {
    let baseline = pipeline(src, false, false);
    let simp_only = pipeline(src, true, false);
    let strict_only = pipeline(src, false, true);
    let both = pipeline(src, true, true);
    println!("{label}");
    println!(
        "  baseline           steps {:>10}   time {:?}",
        baseline.0, baseline.1
    );
    println!(
        "  +simplify          steps {:>10}  ({:>+5.1}%)   time {:?}",
        simp_only.0,
        pct(baseline.0, simp_only.0),
        simp_only.1
    );
    println!(
        "  +strict            steps {:>10}  ({:>+5.1}%)   time {:?}",
        strict_only.0,
        pct(baseline.0, strict_only.0),
        strict_only.1
    );
    println!(
        "  +simplify +strict  steps {:>10}  ({:>+5.1}%)   time {:?}",
        both.0,
        pct(baseline.0, both.0),
        both.1
    );
    println!();
}

fn main() {
    println!("(percentages = step-count reduction vs baseline)\n");
    report("add one two", "add one two");
    report("mul two three", "mul two three");
    report("fact three", "fact three");
    report(
        "fact 5",
        "fact (succ (succ (succ (succ (succ zero)))))",
    );
    report(
        "length [1,2,3]",
        "length (cons one (cons two (cons three nil)))",
    );
    report("compose succ succ one", "compose succ succ one");
    report(
        "map succ [1,2]",
        "length (map succ (cons one (cons two nil)))",
    );
    report("if true one two", "if true one two");
}
