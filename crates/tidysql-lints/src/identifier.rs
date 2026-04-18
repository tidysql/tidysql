use tidysql_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

pub(crate) fn is_identifier_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::NakedIdentifier
            | SyntaxKind::Identifier
            | SyntaxKind::QuotedIdentifier
            | SyntaxKind::NakedIdentifierAll
            | SyntaxKind::FunctionNameIdentifier
            | SyntaxKind::PropertyNameIdentifier
            | SyntaxKind::PropertiesNakedIdentifier
            | SyntaxKind::DataTypeIdentifier
            | SyntaxKind::DatetimeTypeIdentifier
            | SyntaxKind::WildcardIdentifier
            | SyntaxKind::WidgetNameIdentifier
            | SyntaxKind::VersionIdentifier
            | SyntaxKind::ProcedureNameIdentifier
            | SyntaxKind::ColumnIndexIdentifierSegment
    )
}

pub(crate) fn strip_identifier_quotes(text: &str) -> &str {
    if text.len() < 2 {
        return text;
    }

    let bytes = text.as_bytes();
    let last = bytes.len() - 1;
    let strip = matches!((bytes[0], bytes[last]), (b'"', b'"') | (b'`', b'`') | (b'[', b']'));

    if strip { &text[1..last] } else { text }
}

pub(crate) fn normalized_identifier(text: &str) -> String {
    strip_identifier_quotes(text).to_ascii_lowercase()
}

pub(crate) fn alias_identifier(alias_expression: &SyntaxNode) -> Option<SyntaxToken> {
    alias_expression
        .children_with_tokens()
        .filter_map(|child| match child {
            SyntaxElement::Token(token) if is_identifier_kind(token.kind()) => Some(token),
            _ => None,
        })
        .next_back()
}

pub(crate) fn first_identifier_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens().find_map(|element| match element {
        SyntaxElement::Token(token) if is_identifier_kind(token.kind()) => Some(token),
        _ => None,
    })
}

pub(crate) fn column_reference_parts(column_reference: &SyntaxNode) -> Vec<SyntaxToken> {
    column_reference
        .descendants_with_tokens()
        .filter_map(|element| match element {
            SyntaxElement::Token(token) if is_identifier_kind(token.kind()) => Some(token),
            _ => None,
        })
        .collect()
}

pub(crate) fn last_column_reference_part(column_reference: &SyntaxNode) -> Option<SyntaxToken> {
    column_reference_parts(column_reference).into_iter().last()
}

pub(crate) fn has_special_characters(
    identifier: &str,
    allow_space: bool,
    additional_allowed: &str,
) -> bool {
    let candidate = strip_identifier_quotes(identifier);
    candidate.chars().any(|ch| {
        !(ch.is_ascii_alphanumeric()
            || ch == '_'
            || (allow_space && ch == ' ')
            || additional_allowed.contains(ch))
    })
}

pub(crate) fn is_keyword(identifier: &str) -> bool {
    matches!(
        strip_identifier_quotes(identifier).to_ascii_uppercase().as_str(),
        "ALL"
            | "AND"
            | "AS"
            | "ASC"
            | "BY"
            | "CASE"
            | "COUNT"
            | "CROSS"
            | "CTE"
            | "DESC"
            | "DISTINCT"
            | "ELSE"
            | "END"
            | "FALSE"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "INNER"
            | "IS"
            | "JOIN"
            | "LEFT"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "RIGHT"
            | "SELECT"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "USING"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
}
