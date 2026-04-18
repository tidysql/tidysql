use tidysql_syntax::SyntaxKind;

use crate::semantic::StatementAnalysis;
use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct UnusedCte;

impl StatementPass for UnusedCte {
    const CODE: &'static str = "unused_cte";
    const TARGET: SyntaxKind = SyntaxKind::WithCompoundStatement;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.unused_cte.level
    }

    fn check(
        ctx: &LintContext<'_>,
        _node: &tidysql_syntax::SyntaxNode,
        analysis: &StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for cte in &analysis.ctes {
            if analysis.cte_usages.contains(&cte.name) {
                continue;
            }
            diagnostics.push(Diagnostic::from_text_range(
                Self::CODE,
                format!("CTE '{}' is never used.", cte.name),
                ctx.config.lints.unused_cte.level,
                cte.token.text_range(),
            ));
        }
    }
}
