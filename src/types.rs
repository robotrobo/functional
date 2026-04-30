#[allow(unused_imports)]
use std::collections::HashMap;

pub type TVarId = u32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Var(TVarId),
    Arrow(Box<Type>, Box<Type>),
}

impl Type {
    pub fn var(id: TVarId) -> Self {
        Type::Var(id)
    }
    pub fn arrow(a: Type, b: Type) -> Self {
        Type::Arrow(Box::new(a), Box::new(b))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub vars: Vec<TVarId>,
    pub ty: Type,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_arrow_type() {
        let t = Type::arrow(Type::var(0), Type::var(0));
        assert_eq!(t, Type::Arrow(Box::new(Type::Var(0)), Box::new(Type::Var(0))));
    }

    #[test]
    fn build_scheme() {
        let s = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::var(0), Type::var(0)),
        };
        assert_eq!(s.vars, vec![0]);
    }
}
