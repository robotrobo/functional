use std::collections::HashMap;
use std::fmt;

use crate::types::{letter_for_index, TVarId, Type};

#[derive(Debug)]
pub enum TypeError {
    Mismatch(Type, Type),
    OccursCheck(TVarId, Type),
    UnboundVar(String),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::Mismatch(a, b) => {
                let mut renames = HashMap::new();
                let mut next = 0;
                collect_renames(a, &mut renames, &mut next);
                collect_renames(b, &mut renames, &mut next);
                write!(
                    f,
                    "cannot unify {} with {}",
                    render(a, &renames),
                    render(b, &renames),
                )
            }
            TypeError::OccursCheck(id, t) => {
                let mut renames = HashMap::new();
                renames.insert(*id, letter_for_index(0));
                let mut next = 1;
                collect_renames(t, &mut renames, &mut next);
                write!(
                    f,
                    "cannot construct infinite type: {} = {}",
                    letter_for_index(0),
                    render(t, &renames),
                )
            }
            TypeError::UnboundVar(name) => write!(f, "unbound variable: {}", name),
        }
    }
}

impl std::error::Error for TypeError {}

/// Walk `t` and assign a fresh letter to each previously-unseen tvar.
/// Order follows the structural traversal so two adjacent types in a
/// Mismatch share consistent renaming.
fn collect_renames(t: &Type, renames: &mut HashMap<TVarId, String>, next: &mut usize) {
    match t {
        Type::Var(id) => {
            renames.entry(*id).or_insert_with(|| {
                let s = letter_for_index(*next);
                *next += 1;
                s
            });
        }
        Type::Arrow(a, b) => {
            collect_renames(a, renames, next);
            collect_renames(b, renames, next);
        }
        Type::Nat => {}
    }
}

fn render(t: &Type, renames: &HashMap<TVarId, String>) -> String {
    match t {
        Type::Var(id) => renames
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("t{}", id)),
        Type::Arrow(a, b) => {
            let a_str = match **a {
                Type::Arrow(_, _) => format!("({})", render(a, renames)),
                _ => render(a, renames),
            };
            format!("{} -> {}", a_str, render(b, renames))
        }
        Type::Nat => "Nat".to_string(),
    }
}
