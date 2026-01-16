use tidysql_config::CapitalisationPolicy;
use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxToken, TextEdit};

use crate::{Diagnostic, LintContext, Severity, TokenLint};

pub(crate) struct KeywordCase;

impl TokenLint for KeywordCase {
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

        let policy = resolve_policy(options.policy, ctx);
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

fn policy_description(policy: CapitalisationPolicy) -> &'static str {
    match policy {
        CapitalisationPolicy::Consistent => "consistent",
        CapitalisationPolicy::Upper => "uppercase",
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake => "lowercase",
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => "capitalised",
        CapitalisationPolicy::Camel => "camelCase",
    }
}

fn is_ignored(text: &str, options: &tidysql_config::KeywordCaseConfig) -> bool {
    options.ignore_words.iter().any(|w| w.eq_ignore_ascii_case(text))
        || options.ignore_words_regex.iter().any(|r| r.is_match(text))
}

fn resolve_policy(policy: CapitalisationPolicy, ctx: &LintContext<'_>) -> CapitalisationPolicy {
    match policy {
        CapitalisationPolicy::Consistent => infer_policy(ctx),
        other => other,
    }
}

fn infer_policy(ctx: &LintContext<'_>) -> CapitalisationPolicy {
    let (upper, lower) = ctx
        .tree
        .root()
        .descendants_with_tokens()
        .filter_map(|el| match el {
            SyntaxElement::Token(t) if t.kind() == SyntaxKind::Keyword => Some(t),
            _ => None,
        })
        .fold((0usize, 0usize), |(upper, lower), token| {
            let text = token.text();
            if is_all_upper(text) {
                (upper + 1, lower)
            } else if is_all_lower(text) {
                (upper, lower + 1)
            } else {
                (upper, lower)
            }
        });

    if upper >= lower { CapitalisationPolicy::Upper } else { CapitalisationPolicy::Lower }
}

fn is_correct_case(text: &str, policy: CapitalisationPolicy) -> bool {
    match policy {
        CapitalisationPolicy::Consistent => true,
        CapitalisationPolicy::Upper => is_all_upper(text),
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake | CapitalisationPolicy::Camel => {
            is_all_lower(text)
        }
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => is_capitalised(text),
    }
}

fn apply_case(text: &str, policy: CapitalisationPolicy) -> String {
    match policy {
        CapitalisationPolicy::Consistent => text.to_string(),
        CapitalisationPolicy::Upper => text.to_ascii_uppercase(),
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake | CapitalisationPolicy::Camel => {
            text.to_ascii_lowercase()
        }
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => capitalise(text),
    }
}

fn is_all_upper(text: &str) -> bool {
    !text.bytes().any(|b| b.is_ascii_lowercase())
}

fn is_all_lower(text: &str) -> bool {
    !text.bytes().any(|b| b.is_ascii_uppercase())
}

fn is_capitalised(text: &str) -> bool {
    let mut bytes = text.bytes();
    let first_ok = bytes.next().is_none_or(|b| b.is_ascii_uppercase());
    let rest_ok = !bytes.any(|b| b.is_ascii_uppercase());
    first_ok && rest_ok
}

fn capitalise(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut bytes = text.bytes();
    if let Some(first) = bytes.next() {
        result.push(first.to_ascii_uppercase() as char);
    }
    for b in bytes {
        result.push(b.to_ascii_lowercase() as char);
    }
    result
}
