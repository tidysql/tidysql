use tidysql_syntax::{SyntaxKind, SyntaxNode};

use crate::doc::*;
use crate::printer::SqlPrinter;

impl SqlPrinter<'_> {
    pub(crate) fn format_major_clause(&self, node: &SyntaxNode) -> DynDoc {
        match node.kind() {
            SyntaxKind::FromClause => self.format_clause_with_keyword(node, &["from"]),
            SyntaxKind::WhereClause => self.format_clause_with_keyword(node, &["where"]),
            SyntaxKind::GroupbyClause => self.format_clause_with_keyword(node, &["group", "by"]),
            SyntaxKind::HavingClause => self.format_clause_with_keyword(node, &["having"]),
            SyntaxKind::OrderbyClause => self.format_clause_with_keyword(node, &["order", "by"]),
            SyntaxKind::LimitClause => self.format_clause_with_keyword(node, &["limit"]),
            SyntaxKind::FetchClause => self.format_clause_with_keyword(node, &["fetch"]),
            SyntaxKind::QualifyClause => self.format_clause_with_keyword(node, &["qualify"]),
            SyntaxKind::IntoClause => self.format_clause_with_keyword(node, &["into"]),
            _ => txt(self.normalized_node(node)),
        }
    }

    fn format_clause_with_keyword(&self, node: &SyntaxNode, keywords: &[&str]) -> DynDoc {
        let head = keywords
            .iter()
            .map(|keyword| self.context.keyword(keyword))
            .collect::<Vec<_>>()
            .join(" ");
        let tail = self.tokens_without_keyword_prefix(node, keywords.len());
        if tail.is_empty() {
            seq([self.leading_comments_doc(node), txt(head)])
        } else {
            self.format_headed_tail(seq([self.leading_comments_doc(node), txt(head)]), tail)
        }
    }
}
