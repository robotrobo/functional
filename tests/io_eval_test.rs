//! Runtime tests for the IO driver. Use a string-based stdin source and
//! a captured stdout sink so we can assert on side effects without touching
//! real I/O.

use std::cell::RefCell;
use std::rc::Rc;

use lc::cbn::{empty_env, whnf, Budget, Value};
use lc::debruijn::to_db;
use lc::eval::inline_defs;
use lc::io_runtime::{run_io, IoSink, IoSource};
use lc::parser::parse_program;
use lc::simplify::simplify;

struct StringSource {
    lines: RefCell<std::vec::IntoIter<String>>,
}

impl StringSource {
    fn new(input: &str) -> Self {
        Self {
            lines: RefCell::new(
                input
                    .lines()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .into_iter(),
            ),
        }
    }
}

impl IoSource for StringSource {
    fn read_line(&self) -> Option<String> {
        self.lines.borrow_mut().next()
    }
}

#[derive(Default)]
struct StringSink {
    buf: RefCell<String>,
}

impl IoSink for StringSink {
    fn writeln_nat(&self, n: u64) {
        self.buf.borrow_mut().push_str(&format!("{}\n", n));
    }
}

fn run(src: &str, stdin: &str) -> (Value, String) {
    let p = parse_program(src).expect("parse");
    let inlined = inline_defs(&p).expect("inline");
    let prepared = simplify(&inlined);
    let db = to_db(&prepared);
    let mut budget = Budget::new(1_000_000);
    let v = whnf(&db, &empty_env(), &mut budget).expect("whnf");
    let action = match v {
        Value::IOAction(a) => a,
        other => panic!("main is not IO: {:?}", other),
    };
    let source = StringSource::new(stdin);
    let sink = Rc::new(StringSink::default());
    let result = run_io(&action, &source, sink.clone(), &mut budget).expect("run_io");
    let out = sink.buf.borrow().clone();
    (result, out)
}

#[test]
fn pure_42_returns_nat_42_no_output() {
    let (v, out) = run("pure 42", "");
    assert!(matches!(v, Value::Nat(42)));
    assert_eq!(out, "");
}

#[test]
fn pure_unit_returns_unit_no_output() {
    let (v, out) = run("pure ()", "");
    assert!(matches!(v, Value::Unit));
    assert_eq!(out, "");
}

#[test]
fn print_seven_writes_seven_returns_unit() {
    let (v, out) = run("print 7", "");
    assert!(matches!(v, Value::Unit));
    assert_eq!(out, "7\n");
}

#[test]
fn bind_pure_succ_chain_returns_nat_2() {
    let (v, out) = run("bind (pure 1) (\\n. pure (succ n))", "");
    assert!(matches!(v, Value::Nat(2)));
    assert_eq!(out, "");
}

#[test]
fn bind_read_print_writes_input_back() {
    let (v, out) = run("bind readNat print", "5\n");
    assert!(matches!(v, Value::Unit));
    assert_eq!(out, "5\n");
}

#[test]
fn target_program_doubles_input() {
    let (v, out) = run("bind readNat (\\n. print (mul n 2))", "21\n");
    assert!(matches!(v, Value::Unit));
    assert_eq!(out, "42\n");
}

#[test]
fn pure_does_not_force_argument() {
    let p = parse_program("pure (fix (\\x. x))").expect("parse");
    let inlined = inline_defs(&p).expect("inline");
    let prepared = simplify(&inlined);
    let db = to_db(&prepared);
    let mut budget = Budget::new(1_000);
    let v = whnf(&db, &empty_env(), &mut budget).expect("whnf should not diverge");
    assert!(matches!(v, Value::IOAction(_)));
}

#[test]
fn read_nat_parse_failure_returns_runtime_error() {
    use lc::error::EvalError;
    let p = parse_program("readNat").expect("parse");
    let inlined = inline_defs(&p).expect("inline");
    let prepared = simplify(&inlined);
    let db = to_db(&prepared);
    let mut budget = Budget::new(10_000);
    let v = whnf(&db, &empty_env(), &mut budget).expect("whnf");
    let action = match v {
        Value::IOAction(a) => a,
        other => panic!("expected IOAction, got {:?}", other),
    };
    let source = StringSource::new("hello\n");
    let sink = Rc::new(StringSink::default());
    let err = run_io(&action, &source, sink, &mut budget).unwrap_err();
    match err {
        EvalError::Runtime(msg) => assert!(
            msg.contains("could not parse"),
            "unexpected message: {}",
            msg,
        ),
        other => panic!("expected Runtime, got {:?}", other),
    }
}

#[test]
fn read_nat_eof_returns_runtime_error() {
    use lc::error::EvalError;
    let p = parse_program("readNat").expect("parse");
    let inlined = inline_defs(&p).expect("inline");
    let prepared = simplify(&inlined);
    let db = to_db(&prepared);
    let mut budget = Budget::new(10_000);
    let v = whnf(&db, &empty_env(), &mut budget).expect("whnf");
    let action = match v {
        Value::IOAction(a) => a,
        other => panic!("expected IOAction, got {:?}", other),
    };
    let source = StringSource::new("");
    let sink = Rc::new(StringSink::default());
    let err = run_io(&action, &source, sink, &mut budget).unwrap_err();
    match err {
        EvalError::Runtime(msg) => assert!(
            msg.contains("end of input"),
            "unexpected message: {}",
            msg,
        ),
        other => panic!("expected Runtime, got {:?}", other),
    }
}
