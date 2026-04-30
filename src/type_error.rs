use thiserror::Error;

use crate::types::Type;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("cannot unify {0:?} with {1:?}")]
    Mismatch(Type, Type),

    #[error("occurs check: cannot construct infinite type t{0} = {1:?}")]
    OccursCheck(u32, Type),

    #[error("unbound variable in type checking: {0}")]
    UnboundVar(String),
}
