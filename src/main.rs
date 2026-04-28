use std::env;
use std::process;

use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

const DEFAULT_STEP_LIMIT: usize = 100_000_000_000;
const PRELUDE_PATH: &str = "lib/prelude.lc";

fn load_prelude() -> Program {
    let src = match std::fs::read_to_string(PRELUDE_PATH) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading prelude {}: {}", PRELUDE_PATH, e);
            eprintln!("(run from project root so {} is reachable)", PRELUDE_PATH);
            process::exit(1);
        }
    };
    match parse_program(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error parsing prelude {}: {}", PRELUDE_PATH, e);
            process::exit(1);
        }
    }
}

fn merge(prelude: Program, user: Program) -> Program {
    // Prelude defs go first (they have no forward references into user code),
    // then user defs (which may reference prelude), then user main.
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    Program {
        defs,
        main: user.main,
    }
}

fn main() {
    // Heavy reduction (e.g. fact 9) builds Expr trees deep enough that the
    // recursive Drop blows the OS-default 8 MB main-thread stack. Run on a
    // worker thread with a generous stack to sidestep this until we make
    // Expr's Drop iterative.
    const WORKER_STACK: usize = 64 * 1024 * 1024; // 64 MB
    let handle = std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(real_main)
        .expect("spawn worker");
    if let Err(panic) = handle.join() {
        eprintln!("worker panicked: {:?}", panic);
        process::exit(1);
    }
}

fn real_main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        let prelude = load_prelude();
        lc::repl::run(prelude.defs);
        return;
    }
    let path = &args[1];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            process::exit(1);
        }
    };
    let user = match parse_program(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    let program = merge(load_prelude(), user);

    if program.main.is_none() {
        // Library file with only defs — print them and exit.
        for d in &program.defs {
            println!("def {} = {}", d.name, print(&d.body));
        }
        return;
    }

    let inlined = match inline_defs(&program) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    match normalize(&inlined, DEFAULT_STEP_LIMIT) {
        Ok(nf) => println!("{}", print(&nf)),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
