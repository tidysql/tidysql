use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxNode, TextEdit};

use crate::{Diagnostic, LintContext, NodePass, Severity};

pub(crate) struct ElseNull;

impl NodePass for ElseNull {
    const CODE: &'static str = "else_null";
    const TARGET: SyntaxKind = SyntaxKind::CaseExpression;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.else_null.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let Some(else_clause) =
            node.children().find(|child| child.kind() == SyntaxKind::ElseClause)
        else {
            return;
        };
        if !else_clause.children_with_tokens().any(|child| {
            matches!(child, SyntaxElement::Token(token) if token.kind() == SyntaxKind::NullLiteral)
        }) && !else_clause.text().trim().eq_ignore_ascii_case("ELSE NULL")
        {
            return;
        }

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "ELSE NULL is redundant.",
                ctx.config.lints.else_null.level,
                else_clause.text_range(),
            )
            .with_fix(Fix::single("Remove ELSE NULL", TextEdit::delete(else_clause.text_range()))),
        );
    }
}
