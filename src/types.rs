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
