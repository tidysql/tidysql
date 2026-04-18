use tidysql_syntax::{SyntaxKind, SyntaxToken};

use crate::identifier::{is_identifier_kind, strip_identifier_quotes};
use crate::{Diagnostic, LintContext, Severity, TokenPass};

pub(crate) struct DisallowNames;

impl TokenPass for DisallowNames {
    const CODE: &'static str = "disallow_names";

    fn matches(kind: SyntaxKind) -> bool {
        is_identifier_kind(kind)
    }

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.disallow_names.level
    }

    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
        let lint = &ctx.config.lints.disallow_names;
        let options = &lint.options;

        if options.names.is_empty() && options.regexes.is_empty() {
            return;
        }

        let candidate = strip_identifier_quotes(token.text());
        if candidate.is_empty() {
            return;
        }

        let is_disallowed = options.names.iter().any(|w| w.eq_ignore_ascii_case(candidate))
            || options.regexes.iter().any(|r| r.is_match(candidate));

        if !is_disallowed {
            return;
        }

        diagnostics.push(Diagnostic::from_text_range(
            Self::CODE,
            format!("Disallowed name: {candidate}."),
            lint.level,
            token.text_range(),
        ));
    }
}
