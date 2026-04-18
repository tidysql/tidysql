use tidysql_config::{CapitalisationPolicy, KeywordCaseConfig, NotEqualStyle};
use tidysql_syntax::{SyntaxElement, SyntaxKind};

use crate::LintContext;

pub(crate) fn policy_description(policy: CapitalisationPolicy) -> &'static str {
    match policy {
        CapitalisationPolicy::Consistent => "consistent",
        CapitalisationPolicy::Upper => "uppercase",
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake => "lowercase",
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => "capitalised",
        CapitalisationPolicy::Camel => "camelCase",
    }
}

pub(crate) fn resolve_keyword_policy(
    policy: CapitalisationPolicy,
    ctx: &LintContext<'_>,
    options: &KeywordCaseConfig,
) -> CapitalisationPolicy {
    match policy {
        CapitalisationPolicy::Consistent => {
            if let Some(cached) = ctx.inferred_keyword_policy.get() {
                return cached;
            }
            let inferred = infer_keyword_policy(ctx, options);
            ctx.inferred_keyword_policy.set(Some(inferred));
            inferred
        }
        other => other,
    }
}

pub(crate) fn resolve_not_equal_style(
    preferred: NotEqualStyle,
    ctx: &LintContext<'_>,
) -> NotEqualStyle {
    match preferred {
        NotEqualStyle::Consistent => {
            if let Some(cached) = ctx.inferred_not_equal_style.get() {
                return cached;
            }
            let inferred = infer_not_equal_style(ctx);
            ctx.inferred_not_equal_style.set(Some(inferred));
            inferred
        }
        other => other,
    }
}

pub(crate) fn apply_case(text: &str, policy: CapitalisationPolicy) -> String {
    match policy {
        CapitalisationPolicy::Consistent => text.to_string(),
        CapitalisationPolicy::Upper => text.to_ascii_uppercase(),
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake | CapitalisationPolicy::Camel => {
            text.to_ascii_lowercase()
        }
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => capitalise(text),
    }
}

pub(crate) fn is_correct_case(text: &str, policy: CapitalisationPolicy) -> bool {
    match policy {
        CapitalisationPolicy::Consistent => true,
        CapitalisationPolicy::Upper => is_all_upper(text),
        CapitalisationPolicy::Lower | CapitalisationPolicy::Snake | CapitalisationPolicy::Camel => {
            is_all_lower(text)
        }
        CapitalisationPolicy::Pascal | CapitalisationPolicy::Capitalise => is_capitalised(text),
    }
}

pub(crate) fn is_all_upper(text: &str) -> bool {
    !text.bytes().any(|b| b.is_ascii_lowercase())
}

pub(crate) fn is_all_lower(text: &str) -> bool {
    !text.bytes().any(|b| b.is_ascii_uppercase())
}

pub(crate) fn capitalise(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut bytes = text.bytes();
    if let Some(first) = bytes.next() {
        result.push(first.to_ascii_uppercase() as char);
    }
    for byte in bytes {
        result.push(byte.to_ascii_lowercase() as char);
    }
    result
}

fn is_capitalised(text: &str) -> bool {
    let mut bytes = text.bytes();
    let first_ok = bytes.next().is_none_or(|byte| byte.is_ascii_uppercase());
    let rest_ok = !bytes.any(|byte| byte.is_ascii_uppercase());
    first_ok && rest_ok
}

fn infer_keyword_policy(
    ctx: &LintContext<'_>,
    options: &KeywordCaseConfig,
) -> CapitalisationPolicy {
    let (upper, lower) = ctx
        .tree
        .root()
        .descendants_with_tokens()
        .filter_map(|element| match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::Keyword => Some(token),
            _ => None,
        })
        .filter(|token| {
            !options.ignore_words.iter().any(|word| word.eq_ignore_ascii_case(token.text()))
                && !options.ignore_words_regex.iter().any(|regex| regex.is_match(token.text()))
        })
        .fold((0usize, 0usize), |(upper, lower), token| {
            if is_all_upper(token.text()) {
                (upper + 1, lower)
            } else if is_all_lower(token.text()) {
                (upper, lower + 1)
            } else {
                (upper, lower)
            }
        });

    if upper >= lower { CapitalisationPolicy::Upper } else { CapitalisationPolicy::Lower }
}

fn infer_not_equal_style(ctx: &LintContext<'_>) -> NotEqualStyle {
    let (angle, bang) = ctx
        .tree
        .root()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::ComparisonOperator)
        .fold((0usize, 0usize), |(angle, bang), node| match node.text().trim() {
            "<>" => (angle + 1, bang),
            "!=" => (angle, bang + 1),
            _ => (angle, bang),
        });

    if angle >= bang { NotEqualStyle::Angle } else { NotEqualStyle::Bang }
}
