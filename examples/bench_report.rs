//! Quick measurement harness: runs each benchmark program with simplify
//! on and off, prints CBN step counts and wall-time. Not asserted — for
//! human inspection.
//!
//! Usage: `cargo run --release --example bench_report`

use lc::ast::Program;
use lc::eval::{inline_defs, normalize_with_steps};
use lc::parser::parse_program;
use lc::simplify::simplify;
use std::time::Instant;

const STEP_LIMIT: usize = 1_000_000_000;

fn pipeline(user_src: &str, do_simplify: bool) -> (usize, std::time::Duration) {
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
    let (_, steps) = normalize_with_steps(&prepared, STEP_LIMIT).unwrap();
    (steps, t.elapsed())
}

fn report(label: &str, src: &str) {
    let (off_steps, off_t) = pipeline(src, false);
    let (on_steps, on_t) = pipeline(src, true);
    let step_pct = 100.0 * (off_steps as f64 - on_steps as f64) / off_steps as f64;
    let time_pct = 100.0
        * (off_t.as_secs_f64() - on_t.as_secs_f64())
        / off_t.as_secs_f64();
    println!(
        "{:<32} steps {:>10} → {:>10}  ({:>6.1}%)   time {:>8.2?} → {:>8.2?}  ({:>6.1}%)",
        label, off_steps, on_steps, step_pct, off_t, on_t, time_pct
    );
}

fn main() {
    println!("{:<32} {:>30}             {:>30}", "program", "CBN steps (off → on)", "wall time (off → on)");
    println!("{}", "-".repeat(120));
    report("add one two", "add one two");
    report("mul two three", "mul two three");
    report("fact three", "fact three");
    report("fact 5", "fact (succ (succ (succ (succ (succ zero)))))");
    report(
        "length [1,2,3]",
        "length (cons one (cons two (cons three nil)))",
    );
    report("compose succ succ one", "compose succ succ one");
    report("map succ [1,2]", "length (map succ (cons one (cons two nil)))");
}
