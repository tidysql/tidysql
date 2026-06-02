use tidysql_syntax::{NodeOrToken, SyntaxKind, SyntaxNode};

use crate::FormatError;
use crate::builders::keyword;
use crate::doc::*;
use crate::printer::SqlPrinter;

impl SqlPrinter<'_> {
    pub(crate) fn format_with_statement(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let ctes = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::CommonTableExpression)
            .map(|cte| self.format_cte(&cte))
            .collect::<Result<Vec<_>, _>>()?;
        let select = node
            .children()
            .find(|child| child.kind() == SyntaxKind::SelectStatement)
            .ok_or_else(|| self.unsupported(node))?;

        let cte_count = ctes.len();
        let cte_docs = ctes
            .into_iter()
            .enumerate()
            .map(|(index, cte)| if index + 1 == cte_count { cte } else { seq([cte, txt(",")]) })
            .collect::<Vec<_>>();
        Ok(seq([
            self.leading_comments_doc(node),
            boxed(group(seq([
                keyword("with", self.context),
                boxed(nest(
                    self.context.indent_width(),
                    seq([boxed(line()), join(|| boxed(line()), cte_docs)]),
                )),
            ]))),
            hard(),
            self.format_select_statement(&select)?,
        ]))
    }

    fn format_cte(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let Some(bracketed) = node.children().find(|child| child.kind() == SyntaxKind::Bracketed)
        else {
            return Ok(txt(self.normalized_node(node)));
        };

        let prefix = node
            .children_with_tokens()
            .take_while(|element| match element {
                NodeOrToken::Node(child) => child.kind() != SyntaxKind::Bracketed,
                NodeOrToken::Token(_) => true,
            })
            .collect::<Vec<_>>();
        let prefix = self.normalized_elements(&prefix);
        let body = bracketed.children().find(|child| {
            matches!(child.kind(), SyntaxKind::SelectStatement | SyntaxKind::WithCompoundStatement)
        });

        let bracketed = if let Some(body) = body {
            seq([
                txt("("),
                boxed(nest(self.context.indent_width(), seq([hard(), self.format_query(&body)?]))),
                hard(),
                txt(")"),
            ])
        } else {
            txt(self.normalized_node(&bracketed))
        };

        Ok(seq([txt(prefix), txt(" "), bracketed]))
    }
}
