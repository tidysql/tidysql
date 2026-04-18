use std::collections::HashSet;

use tidysql_syntax::SyntaxKind;

use crate::semantic::StatementAnalysis;
use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct UniqueTableAlias;

impl StatementPass for UniqueTableAlias {
    const CODE: &'static str = "unique_table_alias";
    const TARGET: SyntaxKind = SyntaxKind::SelectStatement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.unique_table_alias.level
    }

    fn check(
        ctx: &LintContext<'_>,
        _node: &tidysql_syntax::SyntaxNode,
        analysis: &StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut seen = HashSet::new();
        for alias in &analysis.table_aliases {
            if seen.insert(alias.name.clone()) {
                continue;
            }
            diagnostics.push(Diagnostic::from_text_range(
                Self::CODE,
                format!("Table alias '{}' is reused.", alias.name),
                ctx.config.lints.unique_table_alias.level,
                alias.token.text_range(),
            ));
        }
    }
}
