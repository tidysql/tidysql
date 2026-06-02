use tidysql_config::FormatCommaStyle;
use tidysql_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken};

use crate::FormatError;
use crate::builders::{indented_group, keyword};
use crate::comments::Comment;
use crate::doc::*;
use crate::printer::SqlPrinter;
use crate::tokens::is_comment;

impl SqlPrinter<'_> {
    pub(crate) fn format_select_statement(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let select_clause = node
            .children()
            .find(|child| child.kind() == SyntaxKind::SelectClause)
            .ok_or_else(|| self.unsupported(node))?;
        let mut doc = self.format_select_clause(&select_clause);

        let mut seen_select_clause = false;
        let mut pending_comments = Vec::new();
        for element in node.children_with_tokens() {
            match element {
                NodeOrToken::Node(child) if child.kind() == SyntaxKind::SelectClause => {
                    seen_select_clause = true;
                }
                NodeOrToken::Node(child) if seen_select_clause => {
                    doc = self.append_inter_clause_comments(doc, &mut pending_comments);
                    if is_supported_select_clause(child.kind()) {
                        doc = seq([doc, hard(), self.format_major_clause(&child)]);
                    } else {
                        return Err(self.unsupported(&child));
                    }
                }
                NodeOrToken::Token(token) if seen_select_clause => {
                    pending_comments.extend(comments_from_token(&token));
                }
                _ => {}
            }
        }

        doc = self.append_inter_clause_comments(doc, &mut pending_comments);

        Ok(doc)
    }

    fn append_inter_clause_comments(
        &self,
        mut doc: DynDoc,
        comments: &mut Vec<SyntaxToken>,
    ) -> DynDoc {
        for comment in comments.drain(..) {
            if let Some(comment) = Comment::classify(&comment) {
                doc = seq([
                    doc,
                    boxed(nest(
                        self.context.indent_width(),
                        seq([hard(), txt(comment.trimmed_text().to_string())]),
                    )),
                ]);
            }
        }

        doc
    }

    fn format_select_clause(&self, node: &SyntaxNode) -> DynDoc {
        let elements = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::SelectClauseElement)
            .collect::<Vec<_>>();
        let modifiers = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::SelectClauseModifier)
            .map(|child| self.normalized_node(&child))
            .collect::<Vec<_>>();

        let mut head = seq([self.leading_comments_doc(node), keyword("select", self.context)]);
        for modifier in modifiers {
            head = seq([head, txt(" "), txt(modifier)]);
        }

        if elements.is_empty() {
            return self.format_headed_tail(head, self.tokens_without_head_keyword(node));
        }

        let items = elements
            .iter()
            .enumerate()
            .map(|(index, element)| self.format_select_item(index, elements.len(), element))
            .collect::<Vec<_>>();

        let separator = || match self.context.comma_style() {
            FormatCommaStyle::Trailing => boxed(line()),
            FormatCommaStyle::Leading => {
                boxed(flat_alt(seq([txt(","), txt(" ")]), seq([hard(), txt(", ")])))
            }
        };
        let list = join(separator, items);

        indented_group(head, list, self.context.indent_width())
    }

    fn format_select_item(&self, index: usize, len: usize, element: &SyntaxNode) -> DynDoc {
        let expr = self.format_expression_like(element);
        match self.context.comma_style() {
            FormatCommaStyle::Trailing => {
                if index + 1 == len {
                    expr
                } else {
                    seq([expr, txt(",")])
                }
            }
            FormatCommaStyle::Leading => expr,
        }
    }
}

fn comments_from_token(token: &SyntaxToken) -> Vec<SyntaxToken> {
    token
        .leading_trivia()
        .chain(token.trailing_trivia())
        .filter(|trivia| is_comment(trivia.kind()))
        .collect()
}

fn is_supported_select_clause(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FromClause
            | SyntaxKind::WhereClause
            | SyntaxKind::GroupbyClause
            | SyntaxKind::HavingClause
            | SyntaxKind::OrderbyClause
            | SyntaxKind::LimitClause
            | SyntaxKind::FetchClause
            | SyntaxKind::QualifyClause
            | SyntaxKind::IntoClause
    )
}
