use tidysql_syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct RequireOrderBy;

impl StatementPass for RequireOrderBy {
    const CODE: &'static str = "require_order_by";
    const TARGET: SyntaxKind = SyntaxKind::SelectStatement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.require_order_by.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let has_limit_or_offset = node.children().any(limit_or_offset_clause);
        let has_order_by = node.children().any(|child| child.kind() == SyntaxKind::OrderbyClause);
        if !has_limit_or_offset || has_order_by {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            "LIMIT clauses, including LIMIT ... OFFSET, require ORDER BY for deterministic \
             results.",
            ctx.config.lints.require_order_by.level,
            node.text_range(),
        ));
    }
}

fn limit_or_offset_clause(child: SyntaxNode) -> bool {
    if child.kind() != SyntaxKind::LimitClause {
        return false;
    }

    // Standalone OFFSET is not yet parsed into the syntax tree, so this lint
    // currently applies to LIMIT clauses and LIMIT ... OFFSET forms represented
    // as LimitClause.
    child.descendants_with_tokens().any(|element| match element {
        SyntaxElement::Token(token) => {
            let text = token.text();
            text.eq_ignore_ascii_case("limit") || text.eq_ignore_ascii_case("offset")
        }
        SyntaxElement::Node(_) => false,
    })
}
