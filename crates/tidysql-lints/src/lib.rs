use std::ops::Range;

use tidysql_config::Config;
pub use tidysql_config::Severity;
use tidysql_syntax::{
    DialectKind, Fix, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxTree, TextRange,
};

mod disallow_names;
mod explicit_union;
mod keyword_case;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub severity: Severity,
    pub range: Range<usize>,
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        severity: Severity,
        range: Range<usize>,
    ) -> Self {
        Self { code, message: message.into(), severity, range, fix: None }
    }

    pub fn from_text_range(
        code: &'static str,
        message: impl Into<String>,
        severity: Severity,
        range: TextRange,
    ) -> Self {
        Self::new(code, message, severity, text_range_to_range(range))
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

pub(crate) struct LintContext<'a> {
    pub(crate) dialect: DialectKind,
    pub(crate) tree: &'a SyntaxTree,
    pub(crate) config: &'a Config,
}

#[expect(dead_code)]
pub(crate) trait NodeLint {
    const CODE: &'static str;
    const MESSAGE: &'static str;
    const SEVERITY: Severity;
    const TARGET: SyntaxKind;

    fn level(config: &Config) -> Severity;

    fn check(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>);
}

pub(crate) trait TokenLint {
    const CODE: &'static str;

    fn matches(kind: SyntaxKind) -> bool;
    fn level(config: &Config) -> Severity;
    fn check(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>);
}

pub fn run(dialect: DialectKind, tree: &SyntaxTree, config: &Config) -> Vec<Diagnostic> {
    let ctx = LintContext { dialect, tree, config };
    let mut diagnostics = Vec::new();

    for element in tree.root().descendants_with_tokens() {
        match element {
            SyntaxElement::Node(node) => run_node_lints(&ctx, &node, &mut diagnostics),
            SyntaxElement::Token(token) => run_token_lints(&ctx, &token, &mut diagnostics),
        }
    }

    diagnostics
}

fn run_node_lints(ctx: &LintContext<'_>, node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    run_node_lint::<explicit_union::ExplicitUnion>(ctx, node, diagnostics);
}

fn run_token_lints(ctx: &LintContext<'_>, token: &SyntaxToken, diagnostics: &mut Vec<Diagnostic>) {
    run_token_lint::<disallow_names::DisallowNames>(ctx, token, diagnostics);
    run_token_lint::<keyword_case::KeywordCase>(ctx, token, diagnostics);
}

fn run_node_lint<L: NodeLint>(
    ctx: &LintContext<'_>,
    node: &SyntaxNode,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if L::level(ctx.config) == Severity::Allow {
        return;
    }

    if node.kind() == L::TARGET {
        L::check(ctx, node, diagnostics);
    }
}

fn run_token_lint<L: TokenLint>(
    ctx: &LintContext<'_>,
    token: &SyntaxToken,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if L::level(ctx.config) == Severity::Allow {
        return;
    }

    if L::matches(token.kind()) {
        L::check(ctx, token, diagnostics);
    }
}

fn text_range_to_range(range: TextRange) -> Range<usize> {
    range.start().into()..range.end().into()
}
