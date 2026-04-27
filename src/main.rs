use std::env;
use std::process;

use lc::eval::normalize;
use lc::parser::parse_program;
use lc::pretty::print;

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
    match parse_program(&src) {
        Ok(p) => {
            for d in &p.defs {
                println!("def {} = {}", d.name, print(&d.body));
            }
            if let Some(m) = &p.main {
                match normalize(m, 10_000) {
                    Ok(nf) => println!("main = {}", print(&nf)),
                    Err(e) => {
                        eprintln!("error: {}", e);
                        process::exit(1);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
