use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxNode, TextEdit};

use crate::{Diagnostic, LintContext, NodePass, Severity};

pub(crate) struct NullComparison;

impl NodePass for NullComparison {
    const CODE: &'static str = "null_comparison";
    const TARGET: SyntaxKind = SyntaxKind::Expression;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.null_comparison.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let children: Vec<_> = node.children_with_tokens().collect();
        if children.len() != 3 {
            return;
        }
        let operator = match &children[1] {
            SyntaxElement::Node(operator) if operator.kind() == SyntaxKind::ComparisonOperator => {
                operator.text().trim()
            }
            _ => return,
        };
        if !matches!(operator, "=" | "!=" | "<>") {
            return;
        }

        let replacement = if is_null(&children[0]) {
            rewritten(&children[2], operator)
        } else if is_null(&children[2]) {
            rewritten(&children[0], operator)
        } else {
            return;
        };

        let replacement = preserve_leading_space(node, replacement);

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "Comparisons with NULL must use IS or IS NOT.",
                ctx.config.lints.null_comparison.level,
                node.text_range(),
            )
            .with_fix(
                Self::FIX_PHASE,
                Fix::single(
                    "Use IS / IS NOT for NULL comparisons",
                    TextEdit::replace(node.text_range(), replacement),
                ),
            ),
        );
    }
}

fn is_null(element: &SyntaxElement) -> bool {
    matches!(element, SyntaxElement::Token(token) if token.kind() == SyntaxKind::NullLiteral)
}

fn rewritten(subject: &SyntaxElement, operator: &str) -> String {
    let suffix = if operator == "=" { "IS NULL" } else { "IS NOT NULL" };
    let text = match subject {
        SyntaxElement::Node(node) => node.text(),
        SyntaxElement::Token(token) => token.text(),
    };
    format!("{} {}", text.trim(), suffix)
}

fn preserve_leading_space(node: &SyntaxNode, replacement: String) -> String {
    if node.text().starts_with(char::is_whitespace) {
        format!(" {}", replacement)
    } else {
        replacement
    }
}
