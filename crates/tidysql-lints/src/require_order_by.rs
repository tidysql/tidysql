use tidysql_syntax::{SyntaxKind, SyntaxNode};

use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct RequireOrderBy;

impl StatementPass for RequireOrderBy {
    const CODE: &'static str = "require_order_by";
    const TARGET: SyntaxKind = SyntaxKind::SelectStatement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.require_order_by.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let has_limit = node.children().any(|child| child.kind() == SyntaxKind::LimitClause);
        let has_order_by = node.children().any(|child| child.kind() == SyntaxKind::OrderbyClause);
        if !has_limit || has_order_by {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            "LIMIT/OFFSET requires ORDER BY for deterministic results.",
            ctx.config.lints.require_order_by.level,
            node.text_range(),
        ));
    }
}
