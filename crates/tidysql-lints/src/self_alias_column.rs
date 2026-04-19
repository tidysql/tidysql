use tidysql_syntax::{Fix, SyntaxKind, SyntaxNode, TextEdit};

use crate::identifier::{alias_identifier, last_column_reference_part, strip_identifier_quotes};
use crate::{Diagnostic, LintContext, NodePass, Severity};

pub(crate) struct SelfAliasColumn;

impl NodePass for SelfAliasColumn {
    const CODE: &'static str = "self_alias_column";
    const TARGET: SyntaxKind = SyntaxKind::SelectClauseElement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.self_alias_column.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let Some(column_reference) =
            node.children().find(|child| child.kind() == SyntaxKind::ColumnReference)
        else {
            return;
        };
        let Some(alias_expression) =
            node.children().find(|child| child.kind() == SyntaxKind::AliasExpression)
        else {
            return;
        };

        let Some(reference_name) = last_column_reference_part(&column_reference) else { return };
        let Some(alias_name) = alias_identifier(&alias_expression) else { return };
        if strip_identifier_quotes(reference_name.text())
            != strip_identifier_quotes(alias_name.text())
        {
            return;
        }

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "Column should not be self-aliased.",
                ctx.config.lints.self_alias_column.level,
                alias_name.text_range(),
            )
            .with_fix(
                Self::FIX_PHASE,
                Fix::single("Remove self-alias", TextEdit::delete(alias_expression.text_range())),
            ),
        );
    }
}
