use std::fmt;

use tidysql_syntax::DialectKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    NotImplemented,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::NotImplemented => write!(f, "formatting is not yet implemented"),
        }
    }
}

impl std::error::Error for FormatError {}

pub fn format_with_dialect(source: &str, _dialect: DialectKind) -> Result<String, FormatError> {
    let _ = source;
    Err(FormatError::NotImplemented)
}
