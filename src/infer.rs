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

/// Quantify over every type variable that's free in `t` but NOT free in `env`.
/// This is the only place ∀ binders are introduced.
pub fn generalize(env: &TypeEnv, t: Type) -> Scheme {
    let env_ftv = env.ftv();
    let mut quantified: Vec<TVarId> = t.ftv().into_iter().filter(|v| !env_ftv.contains(v)).collect();
    quantified.sort();
    Scheme { vars: quantified, ty: t }
}

/// Replace every quantified variable in the scheme with a fresh tvar. This
/// is how a let-bound polymorphic identifier becomes a monotype at a
/// specific use site.
pub fn instantiate(scheme: &Scheme, fresh: &mut Fresh) -> Type {
    let mut subst = Subst::empty();
    for v in &scheme.vars {
        subst.0.insert(*v, fresh.tvar());
    }
    subst.apply(&scheme.ty)
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

#[cfg(test)]
mod gen_inst_tests {
    use super::*;

    #[test]
    fn generalize_quantifies_unbound_vars() {
        // env empty; type 0 → 0  ⇒  ∀0. 0 → 0
        let env = TypeEnv::empty();
        let t = Type::arrow(Type::var(0), Type::var(0));
        let s = generalize(&env, t);
        assert_eq!(s.vars, vec![0]);
    }

    #[test]
    fn generalize_skips_env_bound_vars() {
        // env mentions tvar 1; type 1 → 0 — only 0 should be quantified.
        let env_scheme = Scheme {
            vars: vec![],
            ty: Type::var(1),
        };
        let env = TypeEnv::empty().insert("x", env_scheme);
        let t = Type::arrow(Type::var(1), Type::var(0));
        let s = generalize(&env, t);
        assert_eq!(s.vars, vec![0]);
    }

    #[test]
    fn instantiate_renames_bound_vars_to_fresh() {
        // ∀a. a → a — instantiate twice; the two fresh tvars must differ.
        let scheme = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::var(0), Type::var(0)),
        };
        let mut fresh = Fresh::new();
        let t1 = instantiate(&scheme, &mut fresh);
        let t2 = instantiate(&scheme, &mut fresh);
        assert_ne!(t1, t2);
        if let Type::Arrow(a, b) = &t1 {
            assert_eq!(a, b);
        } else {
            panic!("expected arrow");
        }
    }
}
