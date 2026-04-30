//! End-to-end: load `lib/prelude.lc`, type-check, and report which defs
//! HM accepts. This is part regression test, part documentation: the
//! `print_full_status` test prints a table that captures HM's reach
//! across the existing prelude.

use std::collections::HashMap;
use std::fs;

use lc::infer::infer_program;
use lc::parser::parse_program;

fn typecheck_status() -> Vec<(String, bool)> {
    let src = fs::read_to_string("lib/prelude.lc").expect("read prelude");
    let program = parse_program(&src).expect("parse prelude");
    let types = infer_program(&program);
    types.defs.into_iter().map(|(n, r)| (n, r.is_ok())).collect()
}

fn status_map() -> HashMap<String, bool> {
    typecheck_status().into_iter().collect()
}

#[test]
fn id_typechecks() {
    assert_eq!(status_map().get("id"), Some(&true));
}

#[test]
fn const_typechecks() {
    assert_eq!(status_map().get("const"), Some(&true));
}

#[test]
fn compose_typechecks() {
    assert_eq!(status_map().get("compose"), Some(&true));
}

#[test]
fn fact_typechecks_under_primitives() {
    // After Nat migration, fact = fix (\rec. \n. ifz n 1 (mul n (rec (pred n))))
    // should typecheck cleanly to Nat -> Nat.
    assert_eq!(
        status_map().get("fact"),
        Some(&true),
        "fact (using fix and primitives) must typecheck",
    );
}

#[test]
fn entire_prelude_typechecks() {
    // After strict-mode migration, every definition in the prelude must
    // typecheck under HM. The runtime aborts if any prelude def fails.
    let status = typecheck_status();
    let failures: Vec<&str> = status
        .iter()
        .filter_map(|(n, ok)| if !ok { Some(n.as_str()) } else { None })
        .collect();
    assert!(
        failures.is_empty(),
        "prelude defs that fail to typecheck: {:?}",
        failures,
    );
}

#[test]
fn print_full_status() {
    // Diagnostic — always passes. Run with `--nocapture` for the table.
    for (name, ok) in typecheck_status() {
        println!("{:>12} : {}", name, if ok { "OK" } else { "TYPE ERROR" });
    }
}
