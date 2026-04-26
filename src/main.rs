use lc::{ast::Expr, eval::naive_reduce_step, pretty::print};

fn main() {
    println!("Hello, world!");

    let e = Expr::app(
        Expr::abs("x", Expr::var("x")),
        Expr::abs("y", Expr::var("y")),
    );
    let res = naive_reduce_step(&e);

    match res {
        Some(res) => println!("{}", print(&res)),
        None => println!("Found none"),
    };
}
