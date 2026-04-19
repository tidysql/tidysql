use std::collections::HashSet;

use tidysql_syntax::{Fix, SyntaxKind, TextEdit};

use crate::semantic::StatementAnalysis;
use crate::{Diagnostic, LintContext, SemanticPass, Severity};

pub(crate) struct UnusedTableAlias;

impl SemanticPass for UnusedTableAlias {
    const CODE: &'static str = "unused_table_alias";
    const TARGET: SyntaxKind = SyntaxKind::SelectStatement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.unused_table_alias.level
    }

    fn check(
        ctx: &LintContext<'_>,
        _node: &tidysql_syntax::SyntaxNode,
        analysis: &StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let used = analysis
            .qualified_references
            .iter()
            .map(|reference| reference.qualifier.clone())
            .collect::<HashSet<_>>();

        for alias in &analysis.table_aliases {
            if used.contains(&alias.name) {
                continue;
            }
            diagnostics.push(
                Diagnostic::from_text_range(
                    Self::CODE,
                    format!("Table alias '{}' is never used.", alias.name),
                    ctx.config.lints.unused_table_alias.level,
                    alias.token.text_range(),
                )
                .with_fix(
                    Self::FIX_PHASE,
                    Fix::single(
                        "Remove unused table alias",
                        TextEdit::delete(alias.alias_expression.text_range()),
                    ),
                ),
            );
        }
    }
}
