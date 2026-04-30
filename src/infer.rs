use std::collections::{HashMap, HashSet};

use crate::types::{Scheme, Subst, TVarId, Type};

#[derive(Clone, Default, Debug)]
pub struct TypeEnv(pub HashMap<String, Scheme>);

impl TypeEnv {
    pub fn empty() -> Self {
        TypeEnv(HashMap::new())
    }

    pub fn insert(&self, name: impl Into<String>, scheme: Scheme) -> Self {
        let mut next = self.0.clone();
        next.insert(name.into(), scheme);
        TypeEnv(next)
    }

    pub fn apply_subst(&self, s: &Subst) -> TypeEnv {
        TypeEnv(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), s.apply_scheme(v)))
                .collect(),
        )
    }

    pub fn ftv(&self) -> HashSet<TVarId> {
        let mut out = HashSet::new();
        for s in self.0.values() {
            out.extend(s.ftv());
        }
        out
    }
}

pub struct Fresh {
    next: TVarId,
}

impl Fresh {
    pub fn new() -> Self {
        Fresh { next: 0 }
    }

    pub fn tvar(&mut self) -> Type {
        let id = self.next;
        self.next += 1;
        Type::Var(id)
    }
}

impl Default for Fresh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_yields_distinct_vars() {
        let mut f = Fresh::new();
        let a = f.tvar();
        let b = f.tvar();
        assert_ne!(a, b);
    }

    #[test]
    fn env_insert_does_not_mutate_original() {
        let e1 = TypeEnv::empty();
        let _e2 = e1.insert(
            "x",
            Scheme {
                vars: vec![],
                ty: Type::var(0),
            },
        );
        assert!(e1.0.is_empty());
    }
}
