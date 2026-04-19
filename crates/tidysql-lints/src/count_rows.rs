use tidysql_config::CountRowsStyle;
use tidysql_syntax::{Fix, SyntaxKind, SyntaxNode, SyntaxToken, TextEdit};

use crate::{Diagnostic, FixPhase, LintContext, NodePass, Severity};

pub(crate) struct CountRows;

impl NodePass for CountRows {
    const CODE: &'static str = "count_rows";
    const TARGET: SyntaxKind = SyntaxKind::Function;
    const FIX_PHASE: FixPhase = FixPhase::Style;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.count_rows.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let Some(function_name) =
            node.descendants_with_tokens().find_map(|element| match element {
                tidysql_syntax::SyntaxElement::Token(token)
                    if token.kind() == SyntaxKind::FunctionNameIdentifier =>
                {
                    Some(token)
                }
                _ => None,
            })
        else {
            return;
        };

        if !function_name.text().eq_ignore_ascii_case("count") {
            return;
        }

        let Some(target) = replacement_target(node) else { return };
        let preferred = ctx.config.lints.count_rows.options.preferred;
        if current_style(&target) == preferred {
            return;
        }

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                format!("Use COUNT({}).", preferred_text(preferred)),
                ctx.config.lints.count_rows.level,
                target.text_range(),
            )
            .with_fix(
                Self::FIX_PHASE,
                Fix::single(
                    format!("Use COUNT({})", preferred_text(preferred)),
                    TextEdit::replace(target.text_range(), preferred_text(preferred)),
                ),
            ),
        );
    }
}

fn replacement_target(function: &SyntaxNode) -> Option<SyntaxToken> {
    let bracketed = function
        .children()
        .find(|child| child.kind() == SyntaxKind::FunctionContents)?
        .children()
        .find(|child| child.kind() == SyntaxKind::Bracketed)?;
    let text = bracketed.text();
    let inner = text.strip_prefix('(')?.strip_suffix(')')?.trim();
    if !matches!(inner, "*" | "0" | "1") {
        return None;
    }

    bracketed.descendants_with_tokens().find_map(|element| match element {
        tidysql_syntax::SyntaxElement::Token(token)
            if (token.kind() == SyntaxKind::Star || token.kind() == SyntaxKind::NumericLiteral)
                && token.text().trim() == inner =>
        {
            Some(token)
        }
        _ => None,
    })
}

fn current_style(token: &SyntaxToken) -> CountRowsStyle {
    match token.text() {
        "*" => CountRowsStyle::Star,
        "0" => CountRowsStyle::Zero,
        _ => CountRowsStyle::One,
    }
}

fn preferred_text(style: CountRowsStyle) -> &'static str {
    match style {
        CountRowsStyle::Star => "*",
        CountRowsStyle::One => "1",
        CountRowsStyle::Zero => "0",
    }
}
