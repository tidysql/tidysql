use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxNode, TextEdit};

use crate::{Diagnostic, LintContext, NodePass, Severity};

pub(crate) struct DistinctParentheses;

impl NodePass for DistinctParentheses {
    const CODE: &'static str = "distinct_parentheses";
    const TARGET: SyntaxKind = SyntaxKind::SelectClause;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.distinct_parentheses.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let has_distinct = node.descendants_with_tokens().any(|element| match element {
            SyntaxElement::Token(token) => token.text().eq_ignore_ascii_case("distinct"),
            _ => false,
        });
        if !has_distinct {
            return;
        }

        let Some(first_element) =
            node.children().find(|child| child.kind() == SyntaxKind::SelectClauseElement)
        else {
            return;
        };
        let Some(bracketed) =
            first_element.descendants().find(|child| child.kind() == SyntaxKind::Bracketed)
        else {
            return;
        };
        let inner = bracketed
            .text()
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim()
            .to_string();

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "DISTINCT should not wrap the first select target in parentheses.",
                ctx.config.lints.distinct_parentheses.level,
                bracketed.text_range(),
            )
            .with_fix(Fix::single(
                "Remove DISTINCT parentheses",
                TextEdit::replace(bracketed.text_range(), format!(" {}", inner)),
            )),
        );
    }
}
