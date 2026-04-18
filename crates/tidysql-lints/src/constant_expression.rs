use tidysql_syntax::{SyntaxKind, SyntaxNode};

use crate::semantic::StatementAnalysis;
use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct ConstantExpression;

impl StatementPass for ConstantExpression {
    const CODE: &'static str = "constant_expression";
    const TARGET: SyntaxKind = SyntaxKind::Expression;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.constant_expression.level
    }

    fn check(
        ctx: &LintContext<'_>,
        node: &SyntaxNode,
        _analysis: &StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let children: Vec<_> = node.children().collect();
        if children.len() != 3 || children[1].kind() != SyntaxKind::ComparisonOperator {
            return;
        }
        let left = &children[0];
        let right = &children[2];
        let is_self_compare =
            left.kind() == SyntaxKind::ColumnReference && left.text().trim() == right.text().trim();
        let is_literal_compare = left.kind() == SyntaxKind::NumericLiteral
            && right.kind() == SyntaxKind::NumericLiteral
            && left.text().trim() == right.text().trim();
        if !is_self_compare && !is_literal_compare {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            "Expression is constant or self-comparing.",
            ctx.config.lints.constant_expression.level,
            node.text_range(),
        ));
    }
}
