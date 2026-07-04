//! Diagnostics shared across the parse/validate pipeline.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub line: usize,
    pub msg: String,
}

impl Diag {
    pub fn new(line: usize, msg: impl Into<String>) -> Self {
        Diag {
            line,
            msg: msg.into(),
        }
    }
}
