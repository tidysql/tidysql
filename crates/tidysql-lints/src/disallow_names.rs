use tidysql_syntax::{SyntaxKind, SyntaxToken};

use crate::{Diagnostic, LintContext, Severity, TokenLint};

pub(crate) struct DisallowNames;

impl TokenLint for DisallowNames {
    const CODE: &'static str = "disallow_names";
    const MESSAGE: &'static str = "Disallowed name.";
    const SEVERITY: Severity = Severity::Warn;

    fn matches(kind: SyntaxKind) -> bool {
        !matches!(kind, SyntaxKind::Comment | SyntaxKind::InlineComment | SyntaxKind::BlockComment)
    }

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.disallow_names.level
    }

    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
        if ctx.config.lints.disallow_names.options.names.is_empty()
            && ctx.config.lints.disallow_names.options.regexes.is_empty()
        {
            return;
        }

        let raw = token.text();
        let candidate = strip_identifier_quotes(raw);

        if candidate.is_empty() {
            return;
        }

        let name_match = ctx
            .config
            .lints
            .disallow_names
            .options
            .names
            .iter()
            .any(|word| word.eq_ignore_ascii_case(candidate));

        let regex_match = ctx
            .config
            .lints
            .disallow_names
            .options
            .regexes
            .iter()
            .any(|regex| regex.is_match(candidate));

        if !name_match && !regex_match {
            return;
        }

        let range = token.text_range();
        let message = format!("Disallowed name: {candidate}.");
        let severity = ctx.config.lints.disallow_names.level;

        diagnostics.push(Diagnostic::from_text_range(Self::CODE, message, severity, range));
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
