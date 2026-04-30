use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::ast::{Def, Program};
use crate::eval::{inline_defs, normalize};
use crate::parser::parse_program;
use crate::pretty::print;
use crate::simplify::simplify;

const STEP_LIMIT: usize = 1_000_000;

pub fn run(initial_defs: Vec<Def>) {
    let mut env: Vec<Def> = initial_defs;
    let mut typecheck_on = true;
    let mut rl = DefaultEditor::new().expect("readline");
    println!("lc — typed λ-calculus REPL");
    println!("type :help for commands, :quit or Ctrl-D to exit");

    loop {
        let readline = rl.readline("λ> ");
        match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if let Some(rest) = trimmed.strip_prefix(':') {
                    handle_command(rest, &env, &mut typecheck_on);
                    if rest.trim() == "quit" || rest.trim() == "q" {
                        break;
                    }
                    continue;
                }

                evaluate(trimmed, &mut env, typecheck_on);
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => break,
            Err(e) => {
                eprintln!("readline error: {}", e);
                break;
            }
        }
    }
}

fn handle_command(cmd: &str, env: &[Def], typecheck_on: &mut bool) {
    let cmd = cmd.trim();
    match cmd {
        "quit" | "q" => {}
        "help" | "h" => {
            println!(":help                 show this");
            println!(":env                  list current definitions");
            println!(":typecheck on|off     toggle strict type checking (default: on)");
            println!(":quit                 exit");
            println!("def <name> = <expr>   add a definition");
            println!("<expr>                evaluate an expression");
        }
        "env" => {
            for d in env {
                println!("def {} = {}", d.name, print(&d.body));
            }
        }
        "typecheck on" => {
            *typecheck_on = true;
            println!("typecheck: on (strict — type errors block evaluation)");
        }
        "typecheck off" => {
            *typecheck_on = false;
            println!("typecheck: off (advisory — type errors are reported but ignored)");
        }
        "typecheck" => {
            println!(
                "typecheck: {}",
                if *typecheck_on { "on" } else { "off" }
            );
        }
        other => eprintln!("unknown command: :{}", other),
    }
}

fn evaluate(line: &str, env: &mut Vec<Def>, typecheck_on: bool) {
    let parsed = match parse_program(line) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    // Type-check. Build a temporary Program with the existing env + newly
    // parsed defs and infer; print types only for the new ones and main.
    // In strict mode, a type error blocks `def` insertion and `main`
    // evaluation. In :typecheck off mode, errors are reported but ignored.
    let new_count = parsed.defs.len();
    let program_for_types = Program {
        defs: env.iter().cloned().chain(parsed.defs.iter().cloned()).collect(),
        main: parsed.main.clone(),
    };
    let types = crate::infer::infer_program(&program_for_types);
    let new_start = types.defs.len().saturating_sub(new_count);
    let mut had_type_error = false;
    for (name, res) in &types.defs[new_start..] {
        match res {
            Ok(scheme) => println!("{} : {}", name, scheme),
            Err(e) => {
                println!("{} : (type error: {})", name, e);
                had_type_error = true;
            }
        }
    }
    if let Some(t_res) = &types.main_type {
        match t_res {
            Ok(t) => {
                let mut vars: Vec<_> = t.ftv().into_iter().collect();
                vars.sort();
                let s = crate::types::Scheme {
                    vars,
                    ty: t.clone(),
                };
                println!(": {}", s);
            }
            Err(e) => {
                println!(": (type error: {})", e);
                had_type_error = true;
            }
        }
    }

    if had_type_error && typecheck_on {
        // Strict mode: don't add ill-typed defs, don't evaluate main.
        return;
    }

    // Add new defs to env.
    for d in parsed.defs {
        env.push(d);
    }
    // Evaluate main if present.
    if let Some(main) = parsed.main {
        let program = Program {
            defs: env.clone(),
            main: Some(main),
        };
        match inline_defs(&program) {
            Ok(e) => {
                let prepared = simplify(&e);
                match normalize(&prepared, STEP_LIMIT) {
                    Ok(nf) => println!("{}", print(&nf)),
                    Err(err) => eprintln!("{}", err),
                }
            }
            Err(err) => eprintln!("{}", err),
        }
    }
}
