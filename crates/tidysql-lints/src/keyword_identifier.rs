use tidysql_syntax::SyntaxToken;

use crate::identifier::{is_identifier_kind, is_keyword, strip_identifier_quotes};
use crate::{Diagnostic, LintContext, Severity, TokenPass};

pub(crate) struct KeywordIdentifier;

impl TokenPass for KeywordIdentifier {
    const CODE: &'static str = "keyword_identifier";

    fn matches(kind: tidysql_syntax::SyntaxKind) -> bool {
        is_identifier_kind(kind)
    }

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.keyword_identifier.level
    }

    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
        if matches!(token.parent().kind(), tidysql_syntax::SyntaxKind::FunctionName) {
            return;
        }
        if !is_keyword(token.text()) {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            format!(
                "Avoid using SQL keyword as identifier: {}.",
                strip_identifier_quotes(token.text())
            ),
            ctx.config.lints.keyword_identifier.level,
            token.text_range(),
        ));
    }
}
