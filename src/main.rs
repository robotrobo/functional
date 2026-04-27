use std::env;
use std::process;

use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

const DEFAULT_STEP_LIMIT: usize = 10_000;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: lc <file.lc>");
        process::exit(2);
    }
    let path = &args[1];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            process::exit(1);
        }
    };
    let program = match parse_program(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

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
