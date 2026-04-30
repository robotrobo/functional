use std::collections::{HashMap, HashSet};

use crate::type_error::TypeError;

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

    pub fn ftv(&self) -> HashSet<TVarId> {
        let mut out = HashSet::new();
        self.collect_ftv(&mut out);
        out
    }

    fn collect_ftv(&self, out: &mut HashSet<TVarId>) {
        match self {
            Type::Var(id) => {
                out.insert(*id);
            }
            Type::Arrow(a, b) => {
                a.collect_ftv(out);
                b.collect_ftv(out);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub vars: Vec<TVarId>,
    pub ty: Type,
}

impl Scheme {
    pub fn ftv(&self) -> HashSet<TVarId> {
        let mut tv = self.ty.ftv();
        for v in &self.vars {
            tv.remove(v);
        }
        tv
    }
}

#[derive(Clone, Debug, Default)]
pub struct Subst(pub HashMap<TVarId, Type>);

impl Subst {
    pub fn empty() -> Self {
        Subst(HashMap::new())
    }

    pub fn singleton(id: TVarId, ty: Type) -> Self {
        let mut m = HashMap::new();
        m.insert(id, ty);
        Subst(m)
    }

    /// Apply this substitution to a type, recursively. Variables not bound
    /// by the substitution are left alone; bound ones are replaced and the
    /// result is itself substituted (no need for a fixed-point loop because
    /// `compose` keeps the map idempotent).
    pub fn apply(&self, t: &Type) -> Type {
        match t {
            Type::Var(id) => match self.0.get(id) {
                Some(replacement) => replacement.clone(),
                None => t.clone(),
            },
            Type::Arrow(a, b) => Type::arrow(self.apply(a), self.apply(b)),
        }
    }

    pub fn apply_scheme(&self, s: &Scheme) -> Scheme {
        // Don't substitute the bound (∀-quantified) vars.
        let mut filtered = self.clone();
        for v in &s.vars {
            filtered.0.remove(v);
        }
        Scheme {
            vars: s.vars.clone(),
            ty: filtered.apply(&s.ty),
        }
    }

    /// `self ∘ other` — apply `other` first, then `self`. The result is
    /// idempotent: every value in the resulting map already has `self`
    /// applied, so a single `apply` call suffices.
    pub fn compose(&self, other: &Subst) -> Subst {
        let mut out: HashMap<TVarId, Type> = other
            .0
            .iter()
            .map(|(k, v)| (*k, self.apply(v)))
            .collect();
        for (k, v) in &self.0 {
            out.entry(*k).or_insert_with(|| v.clone());
        }
        Subst(out)
    }
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

#[cfg(test)]
mod subst_tests {
    use super::*;

    #[test]
    fn apply_to_var_replaces() {
        let s = Subst::singleton(0, Type::arrow(Type::var(1), Type::var(1)));
        let result = s.apply(&Type::var(0));
        assert_eq!(result, Type::arrow(Type::var(1), Type::var(1)));
    }

    #[test]
    fn apply_to_unbound_var_is_noop() {
        let s = Subst::singleton(0, Type::var(99));
        assert_eq!(s.apply(&Type::var(7)), Type::var(7));
    }

    #[test]
    fn apply_recurses_under_arrow() {
        let s = Subst::singleton(0, Type::var(1));
        let t = Type::arrow(Type::var(0), Type::arrow(Type::var(0), Type::var(2)));
        let expected = Type::arrow(Type::var(1), Type::arrow(Type::var(1), Type::var(2)));
        assert_eq!(s.apply(&t), expected);
    }

    #[test]
    fn compose_applies_left_after_right() {
        // s1 = {0 → 1}, s2 = {1 → 2}: (s2 ∘ s1)(0) should be 2.
        let s1 = Subst::singleton(0, Type::var(1));
        let s2 = Subst::singleton(1, Type::var(2));
        let composed = s2.compose(&s1);
        assert_eq!(composed.apply(&Type::var(0)), Type::var(2));
    }

    #[test]
    fn apply_scheme_skips_bound_vars() {
        // ∀a. a → a — substituting a (= var 0) must not touch the bound a.
        let scheme = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::var(0), Type::var(0)),
        };
        let s = Subst::singleton(0, Type::var(99));
        assert_eq!(s.apply_scheme(&scheme), scheme);
    }
}

/// Unify two types, returning the most general unifier. Implements the
/// classical Robinson algorithm with occurs check. Applying the returned
/// substitution to either input yields equal types.
pub fn unify(a: &Type, b: &Type) -> Result<Subst, TypeError> {
    match (a, b) {
        (Type::Var(x), Type::Var(y)) if x == y => Ok(Subst::empty()),
        (Type::Var(x), t) | (t, Type::Var(x)) => bind(*x, t),
        (Type::Arrow(a1, b1), Type::Arrow(a2, b2)) => {
            let s1 = unify(a1, a2)?;
            let s2 = unify(&s1.apply(b1), &s1.apply(b2))?;
            Ok(s2.compose(&s1))
        }
    }
}

fn bind(x: TVarId, t: &Type) -> Result<Subst, TypeError> {
    if let Type::Var(y) = t {
        if *y == x {
            return Ok(Subst::empty());
        }
    }
    if t.ftv().contains(&x) {
        return Err(TypeError::OccursCheck(x, t.clone()));
    }
    Ok(Subst::singleton(x, t.clone()))
}

#[cfg(test)]
mod ftv_tests {
    use super::*;

    #[test]
    fn ftv_of_var() {
        assert_eq!(Type::var(3).ftv(), [3].into_iter().collect());
    }

    #[test]
    fn ftv_of_arrow() {
        let t = Type::arrow(Type::var(1), Type::arrow(Type::var(2), Type::var(1)));
        assert_eq!(t.ftv(), [1, 2].into_iter().collect());
    }

    #[test]
    fn scheme_ftv_excludes_bound() {
        // ∀a. a → b — only b is free.
        let s = Scheme {
            vars: vec![0],
            ty: Type::arrow(Type::var(0), Type::var(1)),
        };
        assert_eq!(s.ftv(), [1].into_iter().collect());
    }
}

#[cfg(test)]
mod unify_tests {
    use super::*;

    #[test]
    fn unify_var_with_var() {
        let s = unify(&Type::var(0), &Type::var(1)).unwrap();
        // Either {0 → 1} or {1 → 0} works; verify the result equates them.
        assert_eq!(s.apply(&Type::var(0)), s.apply(&Type::var(1)));
    }

    #[test]
    fn unify_same_var_is_empty() {
        let s = unify(&Type::var(7), &Type::var(7)).unwrap();
        assert!(s.0.is_empty());
    }

    #[test]
    fn unify_var_with_arrow() {
        let arr = Type::arrow(Type::var(1), Type::var(2));
        let s = unify(&Type::var(0), &arr).unwrap();
        assert_eq!(s.apply(&Type::var(0)), arr);
    }

    #[test]
    fn unify_occurs_check_fails() {
        // 0 ~ (0 → 1)
        let arr = Type::arrow(Type::var(0), Type::var(1));
        let err = unify(&Type::var(0), &arr).unwrap_err();
        assert!(matches!(err, TypeError::OccursCheck(0, _)));
    }

    #[test]
    fn unify_arrows_recursive() {
        let lhs = Type::arrow(Type::var(0), Type::var(1));
        let rhs = Type::arrow(Type::var(2), Type::var(3));
        let s = unify(&lhs, &rhs).unwrap();
        assert_eq!(s.apply(&lhs), s.apply(&rhs));
    }

    #[test]
    fn unify_chained() {
        // (0 → 0) ~ (1 → 2) — must force 1 = 2.
        let lhs = Type::arrow(Type::var(0), Type::var(0));
        let rhs = Type::arrow(Type::var(1), Type::var(2));
        let s = unify(&lhs, &rhs).unwrap();
        assert_eq!(s.apply(&Type::var(1)), s.apply(&Type::var(2)));
    }
}
