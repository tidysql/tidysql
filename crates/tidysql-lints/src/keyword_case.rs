use tidysql_syntax::{Fix, SyntaxKind, SyntaxToken, TextEdit};

use crate::casing::{apply_case, is_correct_case, policy_description, resolve_keyword_policy};
use crate::{Diagnostic, LintContext, Severity, TokenPass};

pub(crate) struct KeywordCase;

impl TokenPass for KeywordCase {
    const CODE: &'static str = "keyword_case";

    fn matches(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Keyword
    }

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.keyword_case.level
    }

    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
        let options = &ctx.config.lints.keyword_case.options;
        let text = token.text();

        if is_ignored(text, options) {
            return;
        }

        let policy = resolve_keyword_policy(options.policy, ctx, options);
        if is_correct_case(text, policy) {
            return;
        }

        let fixed = apply_case(text, policy);
        let edit = TextEdit::replace(token.text_range(), fixed);
        let fix = Fix::single("Fix keyword case", edit);

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                format!("Keywords must be {}.", policy_description(policy)),
                ctx.config.lints.keyword_case.level,
                token.text_range(),
            )
            .with_fix(fix),
        );
    }
}

fn is_ignored(text: &str, options: &tidysql_config::KeywordCaseConfig) -> bool {
    options.ignore_words.iter().any(|w| w.eq_ignore_ascii_case(text))
        || options.ignore_words_regex.iter().any(|r| r.is_match(text))
}
