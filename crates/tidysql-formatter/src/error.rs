use std::fmt;

use tidysql_syntax::{ParseError, SyntaxKind, TextRange};

#[derive(Debug)]
pub enum FormatError {
    Parse(ParseError),
    UnsupportedSyntax { kind: SyntaxKind, range: TextRange },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Parse(error) => write!(f, "{error}"),
            FormatError::UnsupportedSyntax { kind, range } => write!(
                f,
                "formatting does not yet support {kind:?} at bytes {}..{}",
                usize::from(range.start()),
                usize::from(range.end())
            ),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<ParseError> for FormatError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}
