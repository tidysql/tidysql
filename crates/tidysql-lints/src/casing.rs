use tidysql_config::NotEqualStyle;
use tidysql_syntax::SyntaxKind;

use crate::LintContext;

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
