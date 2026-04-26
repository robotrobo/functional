use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Generic(String),
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("free variable referenced: {0}")]
    FreeVariable(String),

    #[error("reduction step limit ({0}) exceeded")]
    StepLimitExceeded(usize),
}
