use std::collections::HashSet;

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

impl StatementAnalysis {
    pub(crate) fn build(scope: &SyntaxNode) -> Self {
        let mut analysis = Self::default();

        for node in scope.descendants() {
            match node.kind() {
                SyntaxKind::AliasExpression => {
                    let Some(parent) = node.parent() else { continue };
                    let Some(token) = alias_identifier(&node) else { continue };
                    let binding = AliasBinding {
                        name: normalized_identifier(token.text()),
                        token,
                        alias_expression: node.clone(),
                    };
                    match parent.kind() {
                        SyntaxKind::FromExpressionElement => analysis.table_aliases.push(binding),
                        SyntaxKind::SelectClauseElement => analysis.column_aliases.push(binding),
                        _ => {}
                    }
                }
                SyntaxKind::ColumnReference => {
                    let parts = column_reference_parts(&node);
                    if parts.len() >= 2 {
                        analysis.qualified_references.push(QualifiedReference {
                            qualifier: normalized_identifier(parts[0].text()),
                        });
                    }
                }
                SyntaxKind::SetExpression => {
                    let arities = node
                        .children()
                        .filter(|child| child.kind() == SyntaxKind::SelectStatement)
                        .map(|select| select_target_count(&select))
                        .collect::<Vec<_>>();
                    if arities.len() > 1 {
                        analysis.set_expression_arities.push(arities);
                    }
                }
                _ => {}
            }
        }

        if scope.kind() == SyntaxKind::WithCompoundStatement {
            analysis.ctes = cte_definitions(scope);
            analysis.cte_usages = cte_usages(scope);
        }

        analysis
    }
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
    let trailing_children = with_statement
        .children()
        .filter(|child| child.kind() != SyntaxKind::CommonTableExpression)
        .collect::<Vec<_>>();

    trailing_children
        .into_iter()
        .flat_map(|child| child.descendants().collect::<Vec<_>>())
        .filter(|node| node.kind() == SyntaxKind::TableReference)
        .filter_map(|table_reference| first_identifier_token(&table_reference))
        .map(|token| normalized_identifier(token.text()))
        .collect()
}
