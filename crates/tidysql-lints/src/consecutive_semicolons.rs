use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxToken, TextEdit};

use crate::{Diagnostic, FilePass, LintContext, Severity};

pub(crate) struct ConsecutiveSemicolons;

impl FilePass for ConsecutiveSemicolons {
    const CODE: &'static str = "consecutive_semicolons";

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.consecutive_semicolons.level
    }

    fn check(ctx: &LintContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
        let mut previous: Option<SyntaxToken> = None;

        for token in ctx.tree.root().descendants_with_tokens().filter_map(|element| match element {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::StatementTerminator => {
                Some(token)
            }
            _ => None,
        }) {
            let Some(prev) = previous.clone() else {
                previous = Some(token);
                continue;
            };

            let start = usize::from(prev.text_range().end());
            let end = usize::from(token.text_range().start());
            let between = &ctx.tree.text()[start..end];
            if !between.trim().is_empty() {
                previous = Some(token);
                continue;
            }

            diagnostics.push(
                Diagnostic::from_text_range(
                    Self::CODE,
                    "Consecutive semicolons create empty statements.",
                    ctx.config.lints.consecutive_semicolons.level,
                    token.text_range(),
                )
                .with_fix(Fix::single(
                    "Remove duplicate semicolon",
                    TextEdit::delete(token.text_range()),
                )),
            );
            previous = Some(token);
        }
    }
}
