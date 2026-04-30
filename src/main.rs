use std::env;
use std::process;

use lc::ast::Program;
use lc::eval::{inline_defs, normalize_with_options};
use lc::parser::parse_program;
use lc::pretty::print;
use lc::simplify::simplify;

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
    let raw: Vec<String> = env::args().collect();
    let mut no_simplify = false;
    let mut no_strict_eval = false;
    let mut no_typecheck = false;
    let mut args: Vec<String> = Vec::with_capacity(raw.len());
    for a in raw {
        match a.as_str() {
            "--no-simplify" => no_simplify = true,
            // Disables strictness analysis in the evaluator (forces every
            // β-step to use a lazy thunk). Performance toggle, not a
            // type-system toggle.
            "--no-strict" => no_strict_eval = true,
            // Disables HM type checking. By default, ill-typed programs
            // are rejected before evaluation.
            "--no-typecheck" => no_typecheck = true,
            _ => args.push(a),
        }
    }
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
    let prelude_def_count = load_prelude().defs.len();
    let program = merge(load_prelude(), user);

    // Type-check. Print user-supplied def types and main type to stderr.
    // In strict mode (default), abort on any type error in user code.
    // The prelude is pre-vetted; if it ever stops typechecking, that's
    // a project bug — surface it as a runtime panic via load_prelude
    // (the dedicated test `infer_prelude_test` is the regression gate).
    let mut had_type_error = false;
    let types = lc::infer::infer_program(&program);
    for (name, res) in types.defs.iter().skip(prelude_def_count) {
        match res {
            Ok(scheme) => eprintln!("{} : {}", name, scheme),
            Err(e) => {
                eprintln!("{} : (type error: {})", name, e);
                had_type_error = true;
            }
        }
    }
    if let Some(t_res) = &types.main_type {
        match t_res {
            Ok(t) => {
                let mut vars: Vec<_> = t.ftv().into_iter().collect();
                vars.sort();
                let s = lc::types::Scheme {
                    vars,
                    ty: t.clone(),
                };
                eprintln!(": {}", s);
            }
            Err(e) => {
                eprintln!(": (type error: {})", e);
                had_type_error = true;
            }
        }
    }

    if had_type_error && !no_typecheck {
        eprintln!("aborting: type errors above (re-run with --no-typecheck to evaluate anyway)");
        process::exit(1);
    }

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
    let prepared = if no_simplify { inlined } else { simplify(&inlined) };
    match normalize_with_options(&prepared, DEFAULT_STEP_LIMIT, !no_strict_eval) {
        Ok((nf, _)) => println!("{}", print(&nf)),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
