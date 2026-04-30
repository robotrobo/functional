#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Var(String),
    Abs(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Fix(Box<Expr>),
    NatLit(u64),
    Prim(PrimOp),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimOp {
    Succ,
    Pred,
    Add,
    Sub,
    Mul,
    IfZ,
}

impl PrimOp {
    pub fn arity(&self) -> usize {
        match self {
            PrimOp::Succ | PrimOp::Pred => 1,
            PrimOp::Add | PrimOp::Sub | PrimOp::Mul => 2,
            PrimOp::IfZ => 3,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            PrimOp::Succ => "succ",
            PrimOp::Pred => "pred",
            PrimOp::Add => "add",
            PrimOp::Sub => "sub",
            PrimOp::Mul => "mul",
            PrimOp::IfZ => "ifz",
        }
    }
}

impl Expr {
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Var(name.into())
    }

    pub fn abs(param: impl Into<String>, body: Expr) -> Self {
        Expr::Abs(param.into(), Box::new(body))
    }

    pub fn app(f: Expr, x: Expr) -> Self {
        Expr::App(Box::new(f), Box::new(x))
    }

    pub fn fix(e: Expr) -> Self {
        Expr::Fix(Box::new(e))
    }

    pub fn nat(n: u64) -> Self {
        Expr::NatLit(n)
    }

    pub fn prim(op: PrimOp) -> Self {
        Expr::Prim(op)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub body: Expr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub defs: Vec<Def>,
    pub main: Option<Expr>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_identity_lambda() {
        let e = Expr::abs("x", Expr::var("x"));
        assert_eq!(
            e,
            Expr::Abs("x".to_string(), Box::new(Expr::Var("x".to_string())))
        );
    }

    #[test]
    fn build_application() {
        let e = Expr::app(Expr::var("f"), Expr::var("x"));
        assert_eq!(
            e,
            Expr::App(
                Box::new(Expr::Var("f".to_string())),
                Box::new(Expr::Var("x".to_string()))
            )
        );
    }

    #[test]
    fn program_with_one_def() {
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: None,
        };
        assert_eq!(p.defs.len(), 1);
        assert_eq!(p.defs[0].name, "id");
    }
}
