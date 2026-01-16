use tidysql_syntax::{SyntaxKind, SyntaxToken};

use crate::{Diagnostic, LintContext, Severity, TokenLint};

pub(crate) struct DisallowNames;

impl TokenLint for DisallowNames {
    const CODE: &'static str = "disallow_names";

    fn matches(kind: SyntaxKind) -> bool {
        !matches!(kind, SyntaxKind::Comment | SyntaxKind::InlineComment | SyntaxKind::BlockComment)
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

fn strip_identifier_quotes(text: &str) -> &str {
    if text.len() < 2 {
        return text;
    }

    let bytes = text.as_bytes();
    let last = bytes.len() - 1;

    let strip = matches!((bytes[0], bytes[last]), (b'"', b'"') | (b'`', b'`') | (b'[', b']'));

    if strip { &text[1..last] } else { text }
}
