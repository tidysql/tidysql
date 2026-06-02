use tidysql_config::Format;
use tidysql_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use crate::builders::{comments_doc, headed_tail};
use crate::context::SqlFormatContext;
use crate::doc::*;
use crate::tokens::{
    format_print_tokens_doc, is_comment, is_layout_token, node_tokens, normalize_print_tokens,
    print_tokens,
};
use crate::{FormatError, FormatMode};

mod clause;
mod expression;
mod select;
mod with;

pub(crate) struct SqlPrinter<'a> {
    context: SqlFormatContext<'a>,
}

impl<'a> SqlPrinter<'a> {
    pub(crate) fn new(config: &'a Format, mode: FormatMode) -> Self {
        Self { context: SqlFormatContext::new(config, mode) }
    }

    pub(crate) fn format_root(&self, root: &SyntaxNode) -> Result<DynDoc, FormatError> {
        if root.text().trim().is_empty() {
            return Ok(txt(root.text()));
        }

        let mut saw_statement = false;
        let mut docs = Vec::new();
        for element in root.children_with_tokens() {
            match element {
                NodeOrToken::Node(node) if node.kind() == SyntaxKind::Statement => {
                    saw_statement = true;
                    docs.push(self.format_statement(&node)?);
                }
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::StatementTerminator => {
                    if let Some(previous) = docs.pop() {
                        docs.push(seq([previous, txt(token.text())]));
                    } else if self.context.mode() == FormatMode::Pragmatic {
                        docs.push(txt(token.text()));
                    } else {
                        return Err(FormatError::UnsupportedSyntax {
                            kind: token.kind(),
                            range: token.text_range(),
                        });
                    }
                }
                NodeOrToken::Token(token)
                    if is_layout_token(token.kind()) || token.kind() == SyntaxKind::EndOfFile => {}
                NodeOrToken::Node(node) if self.context.mode() == FormatMode::Pragmatic => {
                    docs.push(txt(node.text().to_string()));
                }
                NodeOrToken::Token(token) if self.context.mode() == FormatMode::Pragmatic => {
                    docs.push(txt(token.text().to_string()));
                }
                NodeOrToken::Node(node) => return Err(self.unsupported(&node)),
                NodeOrToken::Token(token) => {
                    return Err(FormatError::UnsupportedSyntax {
                        kind: token.kind(),
                        range: token.text_range(),
                    });
                }
            }
        }

        if !saw_statement {
            return match self.context.mode() {
                FormatMode::Pragmatic => Ok(txt(root.text().to_string())),
                FormatMode::Strict => Err(self.unsupported(root)),
            };
        }

        Ok(join(|| txt("\n\n"), docs))
    }

    fn format_statement(&self, statement: &SyntaxNode) -> Result<DynDoc, FormatError> {
        match self.format_statement_strict(statement) {
            Ok(doc) => Ok(doc),
            Err(error @ FormatError::Parse(_)) => Err(error),
            Err(error @ FormatError::UnsupportedSyntax { .. }) => match self.context.mode() {
                FormatMode::Pragmatic => Ok(txt(statement.text().to_string())),
                FormatMode::Strict => Err(error),
            },
        }
    }

    fn format_statement_strict(&self, statement: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let main = statement.children().next().ok_or_else(|| self.unsupported(statement))?;
        let mut doc = match main.kind() {
            SyntaxKind::SelectStatement | SyntaxKind::WithCompoundStatement => {
                self.format_query(&main)?
            }
            _ => return Err(self.unsupported(&main)),
        };

        if statement
            .children_with_tokens()
            .any(|element| matches!(element, NodeOrToken::Token(token) if token.kind() == SyntaxKind::StatementTerminator))
        {
            doc = seq([doc, txt(";")]);
        }

        Ok(doc)
    }

    fn format_query(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        match node.kind() {
            SyntaxKind::SelectStatement => self.format_select_statement(node),
            SyntaxKind::WithCompoundStatement => self.format_with_statement(node),
            _ => Err(self.unsupported(node)),
        }
    }

    fn format_headed_tail(&self, head: DynDoc, tail: Vec<SyntaxToken>) -> DynDoc {
        headed_tail(
            head,
            self.format_tokens_doc(self.print_tokens(tail)),
            self.context.indent_width(),
        )
    }

    fn leading_comments_doc(&self, node: &SyntaxNode) -> DynDoc {
        comments_doc(
            node.first_token()
                .leading_trivia()
                .filter(|token| is_comment(token.kind()))
                .collect::<Vec<_>>(),
        )
    }

    fn normalized_node_without_head_keyword(&self, node: &SyntaxNode) -> String {
        self.normalized_tokens(self.tokens_without_head_keyword(node))
    }

    fn tokens_without_head_keyword(&self, node: &SyntaxNode) -> Vec<SyntaxToken> {
        node_tokens(node)
            .into_iter()
            .skip_while(|token| token.kind() != SyntaxKind::Keyword)
            .skip(1)
            .collect()
    }

    fn normalized_node_without_keyword_prefix(
        &self,
        node: &SyntaxNode,
        keyword_count: usize,
    ) -> String {
        self.normalized_tokens(self.tokens_without_keyword_prefix(node, keyword_count))
    }

    fn tokens_without_keyword_prefix(
        &self,
        node: &SyntaxNode,
        keyword_count: usize,
    ) -> Vec<SyntaxToken> {
        let mut skipped = 0usize;
        node_tokens(node)
            .into_iter()
            .filter(|token| !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile)
            .filter(|token| {
                if skipped < keyword_count && token.kind() == SyntaxKind::Keyword {
                    skipped += 1;
                    false
                } else {
                    true
                }
            })
            .collect()
    }

    fn normalized_node(&self, node: &SyntaxNode) -> String {
        self.normalized_tokens(node_tokens(node))
    }

    fn normalized_elements(&self, elements: &[SyntaxElement]) -> String {
        let tokens = elements
            .iter()
            .flat_map(|element| match element {
                NodeOrToken::Node(node) => node_tokens(node),
                NodeOrToken::Token(token) => vec![token.clone()],
            })
            .collect::<Vec<_>>();
        self.normalized_tokens(tokens)
    }

    fn normalized_tokens(&self, tokens: Vec<SyntaxToken>) -> String {
        normalize_print_tokens(&self.print_tokens(tokens))
    }

    fn print_tokens(&self, tokens: Vec<SyntaxToken>) -> Vec<crate::tokens::PrintToken> {
        print_tokens(tokens, self.context)
    }

    fn format_tokens_doc(&self, tokens: Vec<crate::tokens::PrintToken>) -> DynDoc {
        format_print_tokens_doc(tokens, self.context)
    }

    fn unsupported(&self, node: &SyntaxNode) -> FormatError {
        FormatError::UnsupportedSyntax { kind: node.kind(), range: node.text_range() }
    }
}
