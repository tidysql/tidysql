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
        let Some(first_element) =
            node.children().find(|child| child.kind() == SyntaxKind::SelectClauseElement)
        else {
            return;
        };

        if !has_top_level_distinct_modifier(node, &first_element) {
            return;
        }

        let Some(bracketed) = distinct_target_bracketed(&first_element) else { return };

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
            .with_fix(
                Self::FIX_PHASE,
                Fix::single(
                    "Remove DISTINCT parentheses",
                    TextEdit::replace(bracketed.text_range(), format!(" {}", inner)),
                ),
            ),
        );
    }
}

fn has_top_level_distinct_modifier(select_clause: &SyntaxNode, first_element: &SyntaxNode) -> bool {
    select_clause
        .children()
        .take_while(|child| child != first_element)
        .filter(|child| child.kind() == SyntaxKind::SelectClauseModifier)
        .any(|child| {
            child.descendants_with_tokens().any(|element| match element {
                SyntaxElement::Token(token) => token.text().eq_ignore_ascii_case("distinct"),
                _ => false,
            })
        })
}

fn distinct_target_bracketed(first_element: &SyntaxNode) -> Option<SyntaxNode> {
    let expression =
        first_element.children().find(|child| child.kind() == SyntaxKind::Expression)?;
    let mut expression_children = expression.children();
    let bracketed = expression_children.next()?;

    if bracketed.kind() != SyntaxKind::Bracketed || expression_children.next().is_some() {
        return None;
    }

    if bracketed.descendants().any(|child| child.kind() == SyntaxKind::SelectStatement) {
        return None;
    }

    Some(bracketed)
}
