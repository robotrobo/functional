use lc::parser::parse_program;
use lc::pretty::print;

#[test]
fn round_trip_examples_identity() {
    let src = std::fs::read_to_string("examples/identity.lc").unwrap();
    let parsed = parse_program(&src).expect("parse should succeed");
    assert_eq!(parsed.defs.len(), 2);
    let main = parsed.main.expect("main expected");
    let printed = print(&main);
    assert_eq!(printed, "const (id apple) banana");
}
