//! Test that `infer_program` plus the main-mode IO check classifies
//! programs correctly. We mirror the file-mode check here without
//! invoking the binary.

use lc::ast::Program;
use lc::infer::infer_program;
use lc::parser::parse_program;
use lc::types::{unify, Type};

fn main_type(src: &str) -> Result<Type, String> {
    let p: Program = parse_program(src).map_err(|e| e.to_string())?;
    let types = infer_program(&p);
    match types.main_type {
        Some(Ok(t)) => Ok(t),
        Some(Err(e)) => Err(e.to_string()),
        None => Err("no main".into()),
    }
}

fn is_io(t: &Type) -> bool {
    let mut fresh = lc::infer::Fresh::new();
    let alpha = fresh.tvar();
    unify(t, &Type::IO(Box::new(alpha))).is_ok()
}

#[test]
fn pure_main_is_io() {
    let t = main_type("pure 1").unwrap();
    assert!(is_io(&t));
}

#[test]
fn print_main_is_io() {
    let t = main_type("print 5").unwrap();
    assert!(is_io(&t));
}

#[test]
fn bind_main_is_io() {
    let t = main_type("bind readNat print").unwrap();
    assert!(is_io(&t));
}

#[test]
fn nat_main_is_not_io() {
    let t = main_type("42").unwrap();
    assert!(!is_io(&t));
}

#[test]
fn lambda_main_is_not_io() {
    let t = main_type("\\x. x").unwrap();
    assert!(!is_io(&t));
}
