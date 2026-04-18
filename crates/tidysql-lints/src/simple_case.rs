use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxNode, TextEdit};

use crate::{Diagnostic, LintContext, NodePass, Severity};

pub(crate) struct SimpleCase;

impl NodePass for SimpleCase {
    const CODE: &'static str = "simple_case";
    const TARGET: SyntaxKind = SyntaxKind::CaseExpression;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.simple_case.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let when_clauses = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::WhenClause)
            .collect::<Vec<_>>();
        if when_clauses.len() != 1 {
            return;
        }
        let Some(else_clause) =
            node.children().find(|child| child.kind() == SyntaxKind::ElseClause)
        else {
            return;
        };

        let when_expressions = when_clauses[0]
            .children()
            .filter(|child| child.kind() == SyntaxKind::Expression)
            .collect::<Vec<_>>();
        if when_expressions.len() != 2 {
            return;
        }
        let else_expression =
            else_clause.children().find(|child| child.kind() == SyntaxKind::Expression);
        let Some(else_expression) = else_expression else { return };
        let condition = &when_expressions[0];
        let then_expression = &when_expressions[1];

        let condition_parts = condition.children_with_tokens().collect::<Vec<_>>();
        if condition_parts.len() != 3 {
            return;
        }
        let subject = match &condition_parts[0] {
            SyntaxElement::Node(node) if node.kind() == SyntaxKind::ColumnReference => node,
            _ => return,
        };
        let is_token = matches!(
            &condition_parts[1],
            SyntaxElement::Token(token) if token.text().eq_ignore_ascii_case("is")
        );
        let null_token = matches!(&condition_parts[2], SyntaxElement::Token(token) if token.kind() == SyntaxKind::NullLiteral);
        if !is_token || !null_token || subject.text().trim() != else_expression.text().trim() {
            return;
        }

        let mut replacement =
            format!("COALESCE({}, {})", subject.text().trim(), then_expression.text().trim());
        if node.text().starts_with(char::is_whitespace) {
            replacement.insert(0, ' ');
        }

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "CASE expression can be simplified to COALESCE.",
                ctx.config.lints.simple_case.level,
                node.text_range(),
            )
            .with_fix(Fix::single(
                "Simplify CASE to COALESCE",
                TextEdit::replace(node.text_range(), replacement),
            )),
        );
    }
}
