use tidysql_syntax::SyntaxNode;

use crate::doc::*;
use crate::printer::SqlPrinter;
use crate::tokens::node_tokens;

impl SqlPrinter<'_> {
    pub(crate) fn format_expression_like(&self, node: &SyntaxNode) -> DynDoc {
        self.format_tokens_doc(self.print_tokens(node_tokens(node)))
    }
}
