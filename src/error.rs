use thiserror::Error;

/// A parse error. Contains a pre-rendered, human-readable message
/// (potentially multi-line, with source-context lines and a caret).
/// Build via `parser::render_errors` from chumsky's raw errors.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("free variable referenced: {0}")]
    FreeVariable(String),

    #[error("reduction step limit ({0}) exceeded")]
    StepLimitExceeded(usize),
}
