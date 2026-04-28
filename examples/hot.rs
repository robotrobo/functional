//! Hot-loop benchmark for flamegraph profiling.
//!
//! Runs `cbn::nf` on a non-trivial workload (fact 7 by default) many
//! times so a sampling profiler has plenty of CPU time to collect.
//!
//! Usage:
//!
//!   cargo install flamegraph     # one-time
//!   cargo flamegraph --release --example hot
//!
//! On macOS you may need: `sudo cargo flamegraph --release --example hot`.
//! On Linux you may need to grant perf permissions first.
//!
//! Output: `flamegraph.svg` in the project root. Open in any browser.

use lc::cbn::{self, Budget};
use lc::debruijn;
use lc::eval::inline_defs;
use lc::parser::parse_program;

const ITERATIONS: usize = 5_000;
const WORKLOAD: &str = "fact (succ (succ five))"; // fact 7

fn main() {
    let prelude = std::fs::read_to_string("lib/prelude.lc").expect("prelude");
    let combined = format!("{}\n{}", prelude, WORKLOAD);
    let prog = parse_program(&combined).expect("parse");
    let inlined = inline_defs(&prog).expect("inline");
    let db = debruijn::to_db(&inlined);

    eprintln!("running {} iterations of `{}`...", ITERATIONS, WORKLOAD);
    let t0 = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        let mut budget = Budget::new(10_000_000);
        let r = cbn::nf(&db, &cbn::empty_env(), 0, &mut budget).expect("nf");
        std::hint::black_box(r);
    }
    let elapsed = t0.elapsed();
    eprintln!(
        "done in {:.3?} ({:.3?} per iter)",
        elapsed,
        elapsed / ITERATIONS as u32
    );
}
