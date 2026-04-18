use tidysql_syntax::SyntaxToken;

use crate::identifier::{has_special_characters, is_identifier_kind, strip_identifier_quotes};
use crate::{Diagnostic, LintContext, Severity, TokenPass};

pub(crate) struct IdentifierCharacters;

impl TokenPass for IdentifierCharacters {
    const CODE: &'static str = "identifier_characters";

    fn matches(kind: tidysql_syntax::SyntaxKind) -> bool {
        is_identifier_kind(kind)
    }

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.identifier_characters.level
    }

    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
        if token.parent().kind() == tidysql_syntax::SyntaxKind::FunctionName {
            return;
        }
        let options = &ctx.config.lints.identifier_characters.options;
        if !has_special_characters(
            token.text(),
            options.allow_space,
            &options.additional_allowed_characters,
        ) {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            format!(
                "Identifier contains special characters: {}.",
                strip_identifier_quotes(token.text())
            ),
            ctx.config.lints.identifier_characters.level,
            token.text_range(),
        ));
    }
}
