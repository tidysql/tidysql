use std::collections::HashSet;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use tidysql_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::identifier::{
    alias_identifier, column_reference_parts, first_identifier_token, normalized_identifier,
};

#[derive(Debug, Clone)]
pub(crate) struct AliasBinding {
    pub(crate) name: String,
    pub(crate) token: SyntaxToken,
    pub(crate) alias_expression: SyntaxNode,
}

#[derive(Debug, Clone)]
pub(crate) struct QualifiedReference {
    pub(crate) qualifier: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CteDefinition {
    pub(crate) name: String,
    pub(crate) token: SyntaxToken,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StatementAnalysis {
    pub(crate) table_aliases: Vec<AliasBinding>,
    pub(crate) column_aliases: Vec<AliasBinding>,
    pub(crate) qualified_references: Vec<QualifiedReference>,
    pub(crate) ctes: Vec<CteDefinition>,
    pub(crate) cte_usages: HashSet<String>,
    pub(crate) set_expression_arities: Vec<Vec<usize>>,
}

#[cfg(test)]
static BUILD_COUNT: AtomicUsize = AtomicUsize::new(0);

impl StatementAnalysis {
    pub(crate) fn build(scope: &SyntaxNode) -> Self {
        #[cfg(test)]
        BUILD_COUNT.fetch_add(1, Ordering::Relaxed);

        let mut analysis = Self::default();

        if scope.kind() != SyntaxKind::WithCompoundStatement {
            collect_scope_analysis(scope, &mut analysis);
        }

        if scope.kind() == SyntaxKind::WithCompoundStatement {
            analysis.ctes = cte_definitions(scope);
            analysis.cte_usages = cte_usages(scope);
        }

        analysis
    }
}

fn collect_scope_analysis(scope: &SyntaxNode, analysis: &mut StatementAnalysis) {
    collect_scope_analysis_from_node(scope, scope, analysis);
}

fn collect_scope_analysis_from_node(
    scope: &SyntaxNode,
    node: &SyntaxNode,
    analysis: &mut StatementAnalysis,
) {
    match node.kind() {
        SyntaxKind::AliasExpression => {
            if let (Some(parent), Some(token)) = (node.parent(), alias_identifier(node)) {
                match parent.kind() {
                    SyntaxKind::FromExpressionElement => {
                        analysis.table_aliases.push(alias_binding(node, token))
                    }
                    SyntaxKind::SelectClauseElement => {
                        analysis.column_aliases.push(alias_binding(node, token))
                    }
                    _ => {}
                }
            }
        }
        SyntaxKind::ColumnReference => {
            let parts = column_reference_parts(node);
            if parts.len() >= 2 {
                analysis
                    .qualified_references
                    .push(QualifiedReference { qualifier: normalized_identifier(parts[0].text()) });
            }
        }
        SyntaxKind::SetExpression => {
            if let Some(arities) = set_expression_arities(node) {
                analysis.set_expression_arities.push(arities);
            }
        }
        _ => {}
    }

    for child in node.children() {
        if child != *scope && is_nested_statement_scope(&child) {
            continue;
        }
        collect_scope_analysis_from_node(scope, &child, analysis);
    }
}

fn is_nested_statement_scope(node: &SyntaxNode) -> bool {
    matches!(node.kind(), SyntaxKind::SelectStatement | SyntaxKind::WithCompoundStatement)
}

#[cfg(test)]
pub(crate) fn reset_build_count() {
    BUILD_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn build_count() -> usize {
    BUILD_COUNT.load(Ordering::Relaxed)
}

fn alias_binding(alias_expression: &SyntaxNode, token: SyntaxToken) -> AliasBinding {
    AliasBinding {
        name: normalized_identifier(token.text()),
        token,
        alias_expression: alias_expression.clone(),
    }
}

fn set_expression_arities(node: &SyntaxNode) -> Option<Vec<usize>> {
    let arities = node
        .children()
        .filter(|child| child.kind() == SyntaxKind::SelectStatement)
        .map(|select| select_target_count(&select))
        .collect::<Vec<_>>();
    (arities.len() > 1).then_some(arities)
}

fn select_target_count(select_statement: &SyntaxNode) -> usize {
    select_statement
        .children()
        .find(|child| child.kind() == SyntaxKind::SelectClause)
        .map(|select_clause| {
            select_clause
                .children()
                .filter(|child| child.kind() == SyntaxKind::SelectClauseElement)
                .count()
        })
        .unwrap_or(0)
}

fn cte_definitions(with_statement: &SyntaxNode) -> Vec<CteDefinition> {
    with_statement
        .children()
        .filter(|child| child.kind() == SyntaxKind::CommonTableExpression)
        .filter_map(|cte| {
            let token = first_identifier_token(&cte)?;
            Some(CteDefinition { name: normalized_identifier(token.text()), token })
        })
        .collect()
}

fn cte_usages(with_statement: &SyntaxNode) -> HashSet<String> {
    let mut usages = HashSet::new();

    for child in
        with_statement.children().filter(|child| child.kind() != SyntaxKind::CommonTableExpression)
    {
        for node in child.descendants() {
            if node.kind() != SyntaxKind::TableReference {
                continue;
            }

            if let Some(token) = first_identifier_token(&node) {
                usages.insert(normalized_identifier(token.text()));
            }
        }
    }

    usages
}
