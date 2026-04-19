use tidysql_config::NotEqualStyle;
use tidysql_syntax::{Fix, SyntaxKind, SyntaxNode, TextEdit};

use crate::casing::resolve_not_equal_style;
use crate::{Diagnostic, FixPhase, LintContext, NodePass, Severity};

pub(crate) struct NotEqualStyleRule;

impl NodePass for NotEqualStyleRule {
    const CODE: &'static str = "not_equal_style";
    const TARGET: SyntaxKind = SyntaxKind::ComparisonOperator;
    const FIX_PHASE: FixPhase = FixPhase::Style;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.not_equal_style.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let actual = match node.text().trim() {
            "!=" => NotEqualStyle::Bang,
            "<>" => NotEqualStyle::Angle,
            _ => return,
        };
        let preferred =
            resolve_not_equal_style(ctx.config.lints.not_equal_style.options.preferred, ctx);
        if actual == preferred {
            return;
        }

        let replacement_core = match preferred {
            NotEqualStyle::Angle | NotEqualStyle::Consistent => "<>",
            NotEqualStyle::Bang => "!=",
        };
        let replacement = if node.text().starts_with(char::is_whitespace) {
            format!(" {}", replacement_core)
        } else {
            replacement_core.to_string()
        };
        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                format!("Use {} for not-equal comparisons.", replacement_core),
                ctx.config.lints.not_equal_style.level,
                node.text_range(),
            )
            .with_fix(
                Self::FIX_PHASE,
                Fix::single(
                    format!("Use {}", replacement_core),
                    TextEdit::replace(node.text_range(), replacement),
                ),
            ),
        );
    }
}
