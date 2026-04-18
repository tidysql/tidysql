use std::collections::HashSet;

use tidysql_syntax::SyntaxKind;

use crate::semantic::StatementAnalysis;
use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct UniqueColumnAlias;

impl StatementPass for UniqueColumnAlias {
    const CODE: &'static str = "unique_column_alias";
    const TARGET: SyntaxKind = SyntaxKind::SelectClause;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.unique_column_alias.level
    }

    fn check(
        ctx: &LintContext<'_>,
        _node: &tidysql_syntax::SyntaxNode,
        analysis: &StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut seen = HashSet::new();
        for alias in &analysis.column_aliases {
            if seen.insert(alias.name.clone()) {
                continue;
            }
            diagnostics.push(Diagnostic::from_text_range(
                Self::CODE,
                format!("Column alias '{}' is reused.", alias.name),
                ctx.config.lints.unique_column_alias.level,
                alias.token.text_range(),
            ));
        }
    }
}
