use tidysql_syntax::{Fix, SyntaxElement, SyntaxKind, SyntaxNode, TextEdit, TextSize};

use crate::{Diagnostic, LintContext, Severity, StatementPass};

pub(crate) struct OrderByDirection;

impl StatementPass for OrderByDirection {
    const CODE: &'static str = "order_by_direction";
    const TARGET: SyntaxKind = SyntaxKind::OrderbyClause;

    fn level(config: &tidysql_config::Config) -> Severity {
        config.lints.order_by_direction.level
    }

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
        let items = order_by_items(node);
        if items.len() < 2 {
            return;
        }

        let any_direction = items.iter().any(|item| item.has_direction);
        let all_direction = items.iter().all(|item| item.has_direction);
        if !any_direction || all_direction {
            return;
        }

        let edits = items
            .iter()
            .filter(|item| !item.has_direction)
            .map(|item| TextEdit::insert(item.end, " ASC"))
            .collect::<Vec<_>>();

        diagnostics.push(
            Diagnostic::from_text_range(
                Self::CODE,
                "ORDER BY items must all specify ASC/DESC when any item does.",
                ctx.config.lints.order_by_direction.level,
                node.text_range(),
            )
            .with_fix(Fix::new("Add ASC to ORDER BY items", edits)),
        );
    }
}

struct OrderByItem {
    end: TextSize,
    has_direction: bool,
}

fn order_by_items(order_by_clause: &SyntaxNode) -> Vec<OrderByItem> {
    let mut items = Vec::new();
    let mut seen_by = false;
    let mut current_end = None;
    let mut current_has_direction = false;

    for child in order_by_clause.children_with_tokens() {
        match child {
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::Keyword => {
                if token.text().eq_ignore_ascii_case("by") {
                    seen_by = true;
                    continue;
                }
                if !seen_by {
                    continue;
                }
                if token.text().eq_ignore_ascii_case("asc")
                    || token.text().eq_ignore_ascii_case("desc")
                {
                    current_end = Some(token.text_range().end());
                    current_has_direction = true;
                }
            }
            SyntaxElement::Token(token) if token.kind() == SyntaxKind::Comma && seen_by => {
                if let Some(end) = current_end.take() {
                    items.push(OrderByItem { end, has_direction: current_has_direction });
                }
                current_has_direction = false;
            }
            SyntaxElement::Node(node) if seen_by => {
                current_end = Some(node.text_range().end());
            }
            _ => {}
        }
    }

    if let Some(end) = current_end {
        items.push(OrderByItem { end, has_direction: current_has_direction });
    }

    items
}
