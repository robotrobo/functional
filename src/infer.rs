use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, Program};
use crate::type_error::TypeError;
use crate::types::{unify, Scheme, Subst, TVarId, Type};

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

/// Algorithm W. Returns a substitution and the inferred type. The
/// substitution must be applied to the result type (and to any caller
/// state) for the type to be principal at this point.
pub fn infer_expr(env: &TypeEnv, e: &Expr, fresh: &mut Fresh) -> Result<(Subst, Type), TypeError> {
    match e {
        Expr::Var(name) => {
            let scheme = env
                .0
                .get(name)
                .ok_or_else(|| TypeError::UnboundVar(name.clone()))?;
            Ok((Subst::empty(), instantiate(scheme, fresh)))
        }
        Expr::Abs(param, body) => {
            let alpha = fresh.tvar();
            let scheme = Scheme {
                vars: vec![],
                ty: alpha.clone(),
            };
            let env2 = env.insert(param.clone(), scheme);
            let (s, t_body) = infer_expr(&env2, body, fresh)?;
            let arrow = Type::arrow(s.apply(&alpha), t_body);
            Ok((s, arrow))
        }
        Expr::App(e1, e2) => {
            let (s1, t1) = infer_expr(env, e1, fresh)?;
            let env2 = env.apply_subst(&s1);
            let (s2, t2) = infer_expr(&env2, e2, fresh)?;
            let alpha = fresh.tvar();
            let s3 = unify(&s2.apply(&t1), &Type::arrow(t2, alpha.clone()))?;
            let composed = s3.compose(&s2).compose(&s1);
            let result_ty = s3.apply(&alpha);
            Ok((composed, result_ty))
        }
    }
}

/// Per-def scheme plus the optional main expression's monotype.
#[derive(Debug, Default)]
pub struct ProgramTypes {
    pub defs: Vec<(String, Result<Scheme, TypeError>)>,
    pub main_type: Option<Result<Type, TypeError>>,
}

/// Type-check a program in advisory mode: each def is inferred independently.
/// A def that fails is recorded as an Err and excluded from the environment
/// for subsequent defs/main — references to its name will produce
/// `UnboundVar` later. Main is checked last.
pub fn infer_program(p: &Program) -> ProgramTypes {
    let mut env = TypeEnv::empty();
    let mut fresh = Fresh::new();
    let mut out = ProgramTypes::default();

    for d in &p.defs {
        match infer_expr(&env, &d.body, &mut fresh) {
            Ok((s, t)) => {
                let env_after = env.apply_subst(&s);
                let scheme = generalize(&env_after, s.apply(&t));
                env = env_after.insert(d.name.clone(), scheme.clone());
                out.defs.push((d.name.clone(), Ok(scheme)));
            }
            Err(e) => {
                out.defs.push((d.name.clone(), Err(e)));
            }
        }
    }

    if let Some(main) = &p.main {
        let r = infer_expr(&env, main, &mut fresh).map(|(s, t)| s.apply(&t));
        out.main_type = Some(r);
    }

    out
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

#[cfg(test)]
mod infer_expr_tests {
    use super::*;
    use crate::ast::Expr;

    fn infer(e: &Expr) -> Result<Type, TypeError> {
        let mut fresh = Fresh::new();
        let (s, t) = infer_expr(&TypeEnv::empty(), e, &mut fresh)?;
        Ok(s.apply(&t))
    }

    #[test]
    fn identity_lambda_is_polymorphic() {
        // \x. x  ⇒  α → α
        let e = Expr::abs("x", Expr::var("x"));
        let t = infer(&e).unwrap();
        if let Type::Arrow(a, b) = &t {
            assert_eq!(a, b);
        } else {
            panic!("not an arrow: {:?}", t);
        }
    }

    #[test]
    fn const_lambda_two_distinct_vars() {
        // \x. \y. x  ⇒  α → β → α
        let e = Expr::abs("x", Expr::abs("y", Expr::var("x")));
        let t = infer(&e).unwrap();
        match t {
            Type::Arrow(a, rest) => match *rest {
                Type::Arrow(_b, c) => assert_eq!(*a, *c),
                _ => panic!("expected nested arrow"),
            },
            _ => panic!("expected arrow"),
        }
    }

    #[test]
    fn application_of_identity() {
        // (\x. x) (\y. y)  ⇒  α → α
        let e = Expr::app(
            Expr::abs("x", Expr::var("x")),
            Expr::abs("y", Expr::var("y")),
        );
        let t = infer(&e).unwrap();
        if let Type::Arrow(a, b) = &t {
            assert_eq!(a, b);
        } else {
            panic!();
        }
    }

    #[test]
    fn unbound_variable_errors() {
        let e = Expr::var("nope");
        let err = infer(&e).unwrap_err();
        assert!(matches!(err, TypeError::UnboundVar(_)));
    }

    #[test]
    fn omega_self_application_fails_occurs_check() {
        // \x. x x  ⇒  occurs check
        let e = Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x")));
        let err = infer(&e).unwrap_err();
        assert!(matches!(err, TypeError::OccursCheck(..)));
    }
}

#[cfg(test)]
mod program_tests {
    use super::*;
    use crate::ast::{Def, Expr, Program};

    #[test]
    fn def_uses_polymorphic_id_at_two_types() {
        // def id = \x. x ; main = id (\y. y)
        let p = Program {
            defs: vec![Def {
                name: "id".into(),
                body: Expr::abs("x", Expr::var("x")),
            }],
            main: Some(Expr::app(Expr::var("id"), Expr::abs("y", Expr::var("y")))),
        };
        let types = infer_program(&p);
        assert!(types.defs[0].1.is_ok(), "id should typecheck");
        assert!(
            types.main_type.unwrap().is_ok(),
            "main should typecheck"
        );
    }

    #[test]
    fn ill_typed_def_is_recorded_but_does_not_abort() {
        let p = Program {
            defs: vec![
                Def {
                    name: "bad".into(),
                    body: Expr::abs("x", Expr::app(Expr::var("x"), Expr::var("x"))),
                },
                Def {
                    name: "good".into(),
                    body: Expr::abs("x", Expr::var("x")),
                },
            ],
            main: None,
        };
        let types = infer_program(&p);
        assert!(types.defs[0].1.is_err(), "bad should fail");
        assert!(types.defs[1].1.is_ok(), "good should still succeed");
    }
}
