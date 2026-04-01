use tidysql_syntax::{
    DialectKind, Fix, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextEdit,
};

use crate::{Diagnostic, LintContext, NodeLint, Severity};

pub(crate) struct ExplicitUnion;

impl NodeLint for ExplicitUnion {
    const CODE: &'static str = "explicit_union";
    const MESSAGE: &'static str = "Use UNION DISTINCT or UNION ALL.";
    const SEVERITY: Severity = Severity::Warn;
    const TARGET: SyntaxKind = SyntaxKind::SetOperator;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.explicit_union.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        if !dialect_supports_union(ctx.dialect) {
            return;
        }

        let Some(union_token) = union_token(node) else { return };

        if union_modifier(node).is_some() {
            return;
        }

        let severity = ctx.config.lints.explicit_union.level;
        let mut diagnostic = Diagnostic::from_text_range(
            Self::CODE,
            Self::MESSAGE,
            severity,
            union_token.text_range(),
        );

        if let Some(fix) = build_fix(&union_token) {
            diagnostic = diagnostic.with_fix(fix);
        }

        diagnostics.push(diagnostic);
    }
}

fn dialect_supports_union(dialect: DialectKind) -> bool {
    matches!(
        dialect,
        DialectKind::Ansi
            | DialectKind::Bigquery
            | DialectKind::Clickhouse
            | DialectKind::Databricks
            | DialectKind::Mysql
            | DialectKind::Redshift
            | DialectKind::Snowflake
            | DialectKind::Trino
    )
}

fn union_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|child| match child {
            SyntaxElement::Token(token) => Some(token),
            SyntaxElement::Node(_) => None,
        })
        .find(|token| token.text().eq_ignore_ascii_case("union"))
}

fn union_modifier(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut seen_union = false;

    for child in node.children_with_tokens() {
        let SyntaxElement::Token(token) = child else { continue };

        if !seen_union {
            if token.text().eq_ignore_ascii_case("union") {
                seen_union = true;
            }
            continue;
        }

        if token.text().eq_ignore_ascii_case("all") || token.text().eq_ignore_ascii_case("distinct")
        {
            return Some(token);
        }

        // Any other non-trivia token after UNION means the modifier is absent.
        return None;
    }

    None
}

fn build_fix(union_token: &SyntaxToken) -> Option<Fix> {
    let suffix = if union_token.text().chars().all(|ch| !ch.is_ascii_uppercase()) {
        " distinct"
    } else {
        " DISTINCT"
    };

    let edit = TextEdit::insert(union_token.text_range().end(), suffix);
    Some(Fix::single("Add DISTINCT to UNION", edit))
}
