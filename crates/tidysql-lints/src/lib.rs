use std::cell::Cell;
use std::ops::Range;

pub use tidysql_config::Severity;
use tidysql_config::{Config, NotEqualStyle};
use tidysql_syntax::{
    DialectKind, Fix, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxTree, TextRange,
};

mod casing;
mod consecutive_semicolons;
mod constant_expression;
mod count_rows;
mod disallow_names;
mod distinct_parentheses;
mod else_null;
mod explicit_union;
mod identifier;
mod identifier_characters;
mod keyword_case;
mod keyword_identifier;
pub mod metadata;
mod not_equal_style;
mod null_comparison;
mod order_by_direction;
mod require_order_by;
mod self_alias_column;
mod semantic;
mod simple_case;
mod unique_column_alias;
mod unique_table_alias;
mod unused_cte;
mod unused_table_alias;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub severity: Severity,
    pub range: Range<usize>,
    pub fix: Option<Fix>,
    pub fix_phase: Option<FixPhase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixPhase {
    Structural,
    Style,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        severity: Severity,
        range: Range<usize>,
    ) -> Self {
        Self { code, message: message.into(), severity, range, fix: None, fix_phase: None }
    }

    pub fn from_text_range(
        code: &'static str,
        message: impl Into<String>,
        severity: Severity,
        range: TextRange,
    ) -> Self {
        Self::new(code, message, severity, text_range_to_range(range))
    }

    pub fn with_fix(mut self, phase: FixPhase, fix: Fix) -> Self {
        self.fix = Some(fix);
        self.fix_phase = Some(phase);
        self
    }
}

pub(crate) struct LintContext<'a> {
    pub(crate) dialect: DialectKind,
    pub(crate) tree: &'a SyntaxTree,
    pub(crate) config: &'a Config,
    pub(crate) inferred_keyword_policy: Cell<Option<tidysql_config::CapitalisationPolicy>>,
    pub(crate) inferred_not_equal_style: Cell<Option<NotEqualStyle>>,
}

pub(crate) trait TokenPass {
    const CODE: &'static str;
    const FIX_PHASE: FixPhase = FixPhase::Structural;

    fn matches(kind: SyntaxKind) -> bool;
    fn level(config: &Config) -> Severity;
    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>);
}

pub(crate) trait NodePass {
    const CODE: &'static str;
    const TARGET: SyntaxKind;
    const FIX_PHASE: FixPhase = FixPhase::Structural;

    fn level(config: &Config) -> Severity;
    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>);
}

pub(crate) trait StatementPass {
    const CODE: &'static str;
    const TARGET: SyntaxKind;
    const FIX_PHASE: FixPhase = FixPhase::Structural;

    fn level(config: &Config) -> Severity;
    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>);
}

pub(crate) trait SemanticPass {
    const CODE: &'static str;
    const TARGET: SyntaxKind;
    const FIX_PHASE: FixPhase = FixPhase::Structural;

    fn level(config: &Config) -> Severity;
    fn check(
        ctx: &LintContext<'_>,
        node: &SyntaxNode,
        analysis: &semantic::StatementAnalysis,
        diagnostics: &mut Vec<Diagnostic>,
    );
}

pub(crate) trait FilePass {
    const CODE: &'static str;
    const FIX_PHASE: FixPhase = FixPhase::Structural;

    fn level(config: &Config) -> Severity;
    fn check(ctx: &LintContext<'_>, diagnostics: &mut Vec<Diagnostic>);
}

const fn needs_statement_analysis(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::SelectStatement
            | SyntaxKind::SelectClause
            | SyntaxKind::OrderbyClause
            | SyntaxKind::WithCompoundStatement
            | SyntaxKind::Expression
    )
}

const fn lint_enabled(level: Severity) -> bool {
    !matches!(level, Severity::Allow)
}

pub fn run(dialect: DialectKind, tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let ctx = LintContext {
        dialect,
        tree,
        config,
        inferred_keyword_policy: Cell::new(None),
        inferred_not_equal_style: Cell::new(None),
    };
    let mut diagnostics = Vec::new();

    for element in tree.root().descendants_with_tokens() {
        match element {
            SyntaxElement::Node(node) => {
                run_node_passes(&ctx, &node, &mut diagnostics);
                run_statement_passes(&ctx, &node, &mut diagnostics);
            }
            SyntaxElement::Token(token) => run_token_passes(&ctx, &token, &mut diagnostics),
        }
    }

    run_file_passes(&ctx, &mut diagnostics);
    diagnostics
}

fn run_node_passes(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    run_node_pass::<count_rows::CountRows>(ctx, node, diagnostics);
    run_node_pass::<distinct_parentheses::DistinctParentheses>(ctx, node, diagnostics);
    run_node_pass::<else_null::ElseNull>(ctx, node, diagnostics);
    run_node_pass::<explicit_union::ExplicitUnion>(ctx, node, diagnostics);
    run_node_pass::<not_equal_style::NotEqualStyleRule>(ctx, node, diagnostics);
    run_node_pass::<null_comparison::NullComparison>(ctx, node, diagnostics);
    run_node_pass::<self_alias_column::SelfAliasColumn>(ctx, node, diagnostics);
    run_node_pass::<simple_case::SimpleCase>(ctx, node, diagnostics);
}

fn run_statement_passes(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    run_statement_pass::<constant_expression::ConstantExpression>(ctx, node, diagnostics);
    run_statement_pass::<order_by_direction::OrderByDirection>(ctx, node, diagnostics);
    run_statement_pass::<require_order_by::RequireOrderBy>(ctx, node, diagnostics);

    let analysis = semantic_analysis_for_enabled_passes(ctx, node);
    let Some(analysis) = analysis.as_ref() else {
        return;
    };

    run_semantic_pass::<unique_column_alias::UniqueColumnAlias>(ctx, node, analysis, diagnostics);
    run_semantic_pass::<unique_table_alias::UniqueTableAlias>(ctx, node, analysis, diagnostics);
    run_semantic_pass::<unused_cte::UnusedCte>(ctx, node, analysis, diagnostics);
    run_semantic_pass::<unused_table_alias::UnusedTableAlias>(ctx, node, analysis, diagnostics);
}

fn run_token_passes(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
    run_token_pass::<disallow_names::DisallowNames>(ctx, token, diagnostics);
    run_token_pass::<identifier_characters::IdentifierCharacters>(ctx, token, diagnostics);
    run_token_pass::<keyword_case::KeywordCase>(ctx, token, diagnostics);
    run_token_pass::<keyword_identifier::KeywordIdentifier>(ctx, token, diagnostics);
}

fn run_file_passes(ctx: &LintContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    run_file_pass::<consecutive_semicolons::ConsecutiveSemicolons>(ctx, diagnostics);
}

fn run_node_pass<L: NodePass>(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !lint_enabled(L::level(ctx.config)) || node.kind() != L::TARGET {
        return;
    }

    L::check(ctx, node, diagnostics);
}

fn run_statement_pass<L: StatementPass>(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !lint_enabled(L::level(ctx.config)) || node.kind() != L::TARGET {
        return;
    }

    L::check(ctx, node, diagnostics);
}

fn run_semantic_pass<L: SemanticPass>(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
    analysis: &semantic::StatementAnalysis,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !lint_enabled(L::level(ctx.config)) || node.kind() != L::TARGET {
        return;
    }

    L::check(ctx, node, analysis, diagnostics);
}

fn run_token_pass<L: TokenPass>(
    ctx: &LintContext<'_>,
    token: &SyntaxToken,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !lint_enabled(L::level(ctx.config)) || !L::matches(token.kind()) {
        return;
    }

    L::check(ctx, token, diagnostics);
}

fn run_file_pass<L: FilePass>(ctx: &LintContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !lint_enabled(L::level(ctx.config)) {
        return;
    }

    L::check(ctx, diagnostics);
}

fn semantic_analysis_for_enabled_passes(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
) -> Option<semantic::StatementAnalysis> {
    if !needs_statement_analysis(node.kind()) {
        return None;
    }

    let needs_analysis =
        semantic_pass_enabled::<unique_column_alias::UniqueColumnAlias>(ctx, node.kind())
            || semantic_pass_enabled::<unique_table_alias::UniqueTableAlias>(ctx, node.kind())
            || semantic_pass_enabled::<unused_cte::UnusedCte>(ctx, node.kind())
            || semantic_pass_enabled::<unused_table_alias::UnusedTableAlias>(ctx, node.kind());

    needs_analysis.then(|| semantic::StatementAnalysis::build(node))
}

fn semantic_pass_enabled<L: SemanticPass>(ctx: &LintContext<'_>, kind: SyntaxKind) -> bool {
    lint_enabled(L::level(ctx.config)) && kind == L::TARGET
}

fn text_range_to_range(range: TextRange) -> Range<usize> {
    range.start().into()..range.end().into()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tidysql_syntax::{DialectKind, parse};

    use super::*;

    static BUILD_COUNT_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn skips_statement_analysis_when_only_non_semantic_passes_are_enabled() {
        let _guard = BUILD_COUNT_LOCK.lock().unwrap();
        let config = tidysql_config::Config::from_toml_str(
            r#"
[lints]
constant_expression = { level = "warn" }
unique_column_alias = { level = "allow" }
unique_table_alias = { level = "allow" }
unused_cte = { level = "allow" }
unused_table_alias = { level = "allow" }
"#,
        )
        .unwrap();
        let tree = parse("SELECT * FROM foo LIMIT 10", DialectKind::Ansi).unwrap();

        semantic::reset_build_count();
        let _ = run(DialectKind::Ansi, &tree, &config);

        assert_eq!(semantic::build_count(), 0);
    }

    #[test]
    fn builds_statement_analysis_for_enabled_semantic_passes() {
        let _guard = BUILD_COUNT_LOCK.lock().unwrap();
        let config = tidysql_config::Config::from_toml_str(
            r#"
[lints]
constant_expression = { level = "allow" }
unique_table_alias = { level = "warn" }
unique_column_alias = { level = "allow" }
unused_cte = { level = "allow" }
unused_table_alias = { level = "allow" }
"#,
        )
        .unwrap();
        let tree = parse("SELECT * FROM foo AS t, bar AS t", DialectKind::Ansi).unwrap();

        semantic::reset_build_count();
        let diagnostics = run(DialectKind::Ansi, &tree, &config);

        assert!(semantic::build_count() >= 1);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "unique_table_alias");
    }
}
