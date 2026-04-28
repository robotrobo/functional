use lc::ast::Program;
use lc::eval::{inline_defs, normalize};
use lc::parser::parse_program;
use lc::pretty::print;

fn run_with_prelude(user_src: &str) -> String {
    let prelude_src = std::fs::read_to_string("lib/prelude.lc").unwrap();
    let prelude = parse_program(&prelude_src).unwrap();
    let user = parse_program(user_src).unwrap();
    let mut defs = prelude.defs;
    defs.extend(user.defs);
    let program = Program { defs, main: user.main };
    let inlined = inline_defs(&program).unwrap();
    let nf = normalize(&inlined, 1_000_000).unwrap();
    print(&nf)
}

#[test]
fn add_one_two_is_three() {
    assert_eq!(run_with_prelude("add one two"), "3");
}

#[test]
fn fact_three_via_prelude() {
    assert_eq!(run_with_prelude("fact three"), "6");
}

#[test]
fn list_length_three() {
    assert_eq!(
        run_with_prelude("length (cons one (cons two (cons three nil)))"),
        "3",
    );
}
