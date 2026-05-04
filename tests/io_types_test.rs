//! Type-level tests for the IO monad. Evaluator support comes in later
//! tasks; this file only checks inference.

use lc::ast::Expr;
use lc::infer::{infer_expr, Fresh, TypeEnv};
use lc::parser::parse_expr;
use lc::types::{Scheme, Type};

fn infer_top(src: &str) -> Result<Type, lc::type_error::TypeError> {
    let e = parse_expr(src).expect("parse");
    let mut fresh = Fresh::new();
    let (s, t) = infer_expr(&TypeEnv::empty(), &e, &mut fresh)?;
    Ok(s.apply(&t))
}

fn scheme_for(src: &str) -> Scheme {
    let t = infer_top(src).expect("infer");
    let mut vars: Vec<_> = t.ftv().into_iter().collect();
    vars.sort();
    Scheme { vars, ty: t }
}

#[test]
fn pure_is_polymorphic() {
    let s = scheme_for("pure");
    assert_eq!(format!("{}", s), "forall a. a -> IO a");
}

#[test]
fn pure_one_has_type_io_nat() {
    let t = infer_top("pure 1").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Nat)));
}

#[test]
fn pure_unit_has_type_io_unit() {
    let t = infer_top("pure ()").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Unit)));
}

#[test]
fn read_nat_has_type_io_nat() {
    let t = infer_top("readNat").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Nat)));
}

#[test]
fn print_has_type_nat_to_io_unit() {
    let t = infer_top("print").unwrap();
    assert_eq!(
        t,
        Type::arrow(Type::Nat, Type::IO(Box::new(Type::Unit))),
    );
}

#[test]
fn print_five_has_type_io_unit() {
    let t = infer_top("print 5").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Unit)));
}

#[test]
fn bind_is_polymorphic() {
    let s = scheme_for("bind");
    let printed = format!("{}", s);
    assert_eq!(printed, "forall a b. IO a -> (a -> IO b) -> IO b");
}

#[test]
fn bind_read_print_has_type_io_unit() {
    let t = infer_top("bind readNat print").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Unit)));
}

#[test]
fn bind_pure_succ_chain_typechecks() {
    let t = infer_top("bind (pure 1) (\\n. pure (succ n))").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Nat)));
}

#[test]
fn bind_with_nat_first_arg_fails() {
    let r = infer_top("bind 1 print");
    assert!(r.is_err(), "expected type error, got {:?}", r);
}

#[test]
fn print_with_unit_arg_fails() {
    let r = infer_top("print ()");
    assert!(r.is_err(), "expected type error, got {:?}", r);
}

#[test]
fn target_program_typechecks() {
    let t = infer_top("bind readNat (\\n. print (mul n 2))").unwrap();
    assert_eq!(t, Type::IO(Box::new(Type::Unit)));
}
