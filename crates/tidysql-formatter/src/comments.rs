use tidysql_syntax::{SyntaxKind, SyntaxToken};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Inline,
    Block,
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comment {
    kind: CommentKind,
    text: String,
}

impl Comment {
    pub(crate) fn from_kind_and_text(kind: SyntaxKind, text: String) -> Option<Self> {
        let kind = match kind {
            SyntaxKind::InlineComment => CommentKind::Inline,
            SyntaxKind::BlockComment => CommentKind::Block,
            SyntaxKind::Comment => CommentKind::Plain,
            _ => return None,
        };

        Some(Self { kind, text })
    }

    pub(crate) fn classify(token: &SyntaxToken) -> Option<Self> {
        Self::from_kind_and_text(token.kind(), token.text().to_string())
    }

    pub(crate) fn kind(&self) -> CommentKind {
        self.kind
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn trimmed_text(&self) -> &str {
        self.text.trim_end()
    }
}

pub(crate) fn is_comment_kind(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::InlineComment | SyntaxKind::BlockComment | SyntaxKind::Comment)
}
