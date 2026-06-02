use tidysql_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, WalkEventWithTokens};

use crate::builders::{delimited, keyword};
use crate::comments::{Comment, CommentKind, is_comment_kind};
use crate::context::SqlFormatContext;
use crate::doc::*;

#[derive(Clone)]
pub(crate) struct PrintToken {
    pub(crate) kind: SyntaxKind,
    pub(crate) text: String,
}

impl PrintToken {
    fn new(kind: SyntaxKind, text: String) -> Self {
        Self { kind, text }
    }

    fn comment(&self) -> Option<Comment> {
        Comment::from_kind_and_text(self.kind, self.text.clone())
    }
}

pub(crate) fn print_tokens(
    tokens: Vec<SyntaxToken>,
    context: SqlFormatContext<'_>,
) -> Vec<PrintToken> {
    let mut pieces = Vec::new();
    for token in tokens {
        for trivia in token.leading_trivia() {
            if is_comment_kind(trivia.kind()) {
                pieces.push(PrintToken::new(trivia.kind(), trivia.text().to_string()));
            }
        }
        if !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile {
            pieces.push(PrintToken::new(token.kind(), format_token_text(&token, context)));
        }
        for trivia in token.trailing_trivia() {
            if is_comment_kind(trivia.kind()) {
                pieces.push(PrintToken::new(trivia.kind(), trivia.text().to_string()));
            }
        }
    }
    pieces
}

pub(crate) fn format_print_tokens_doc(
    tokens: Vec<PrintToken>,
    context: SqlFormatContext<'_>,
) -> DynDoc {
    let tokens = trim_print_tokens(&tokens);
    if tokens.is_empty() {
        return boxed(nil());
    }

    if tokens.iter().any(|token| token.comment().is_some()) {
        return format_commented_tokens_doc(tokens, context);
    }

    if starts_with_keyword(tokens, "case") {
        return format_case_tokens_doc(tokens, context);
    }

    if let Some(parts) = split_breakable_tokens(tokens, is_boolean_break_token) {
        return boxed(group(join(
            || boxed(line()),
            parts.into_iter().map(|part| format_print_token_range_doc(part, context)),
        )));
    }

    if let Some(parts) = split_breakable_tokens(tokens, is_join_break_token) {
        return boxed(group(join(
            || boxed(line()),
            parts.into_iter().map(|part| format_print_token_range_doc(part, context)),
        )));
    }

    format_print_token_range_doc(tokens, context)
}

fn format_print_token_range_doc(tokens: &[PrintToken], context: SqlFormatContext<'_>) -> DynDoc {
    let mut docs = Vec::new();
    let mut chunk = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if is_open_bracket(token.kind)
            && let Some(close_index) = find_matching_bracket(tokens, index)
        {
            let needs_space = chunk.last().is_some_and(bracket_needs_leading_space);
            flush_print_chunk(&mut docs, &mut chunk);
            if needs_space {
                docs.push(txt(" "));
            }
            docs.push(format_delimited_tokens_doc(
                &token.text,
                &tokens[index + 1..close_index],
                &tokens[close_index].text,
                context,
            ));
            if tokens.get(close_index + 1).is_some_and(token_needs_space_after_bracket_group) {
                docs.push(txt(" "));
            }
            index = close_index + 1;
            continue;
        }

        chunk.push(token.clone());
        index += 1;
    }

    flush_print_chunk(&mut docs, &mut chunk);
    boxed(group(seq(docs)))
}

fn format_commented_tokens_doc(tokens: &[PrintToken], context: SqlFormatContext<'_>) -> DynDoc {
    let mut lines = Vec::new();
    let mut current = Vec::new();

    for token in tokens {
        match token.comment() {
            Some(comment) if comment.kind() == CommentKind::Inline => {
                let comment = txt(comment.trimmed_text().to_string());
                if current.is_empty() {
                    lines.push(comment);
                } else {
                    lines.push(seq([
                        format_print_token_range_doc(trim_print_tokens(&current), context),
                        txt(" "),
                        comment,
                    ]));
                    current.clear();
                }
            }
            Some(comment) => {
                if !current.is_empty() {
                    lines.push(format_print_token_range_doc(trim_print_tokens(&current), context));
                    current.clear();
                }
                lines.push(txt(comment.trimmed_text().to_string()));
            }
            None => current.push(token.clone()),
        }
    }

    if !current.is_empty() {
        lines.push(format_print_token_range_doc(trim_print_tokens(&current), context));
    }

    join(hard, lines)
}

fn format_delimited_tokens_doc(
    open: &str,
    inner: &[PrintToken],
    close: &str,
    context: SqlFormatContext<'_>,
) -> DynDoc {
    let inner = trim_print_tokens(inner);
    if inner.is_empty() {
        return seq([txt(open), txt(close)]);
    }

    if let Some(parts) = split_breakable_tokens(inner, is_comma_break_token) {
        let docs = parts
            .into_iter()
            .map(|part| format_print_tokens_doc(part.to_vec(), context))
            .collect::<Vec<_>>();
        return delimited(open, join(comma_line, docs), close, context.indent_width());
    }

    boxed(group(seq([txt(open), format_print_tokens_doc(inner.to_vec(), context), txt(close)])))
}

fn format_case_tokens_doc(tokens: &[PrintToken], context: SqlFormatContext<'_>) -> DynDoc {
    let flat = txt(normalize_print_tokens(tokens));
    let end_index = tokens
        .iter()
        .rposition(|token| is_keyword_text(token, "end"))
        .unwrap_or(tokens.len().saturating_sub(1));
    let case_index = tokens.iter().position(|token| is_keyword_text(token, "case")).unwrap_or(0);
    let first_when =
        tokens.iter().position(|token| is_keyword_text(token, "when")).unwrap_or(end_index);
    let subject = trim_print_tokens(&tokens[case_index + 1..first_when]);
    let body_tokens = &tokens[first_when..end_index];
    let tail = trim_print_tokens(&tokens[end_index + 1..]);

    let mut head = keyword("case", context);
    if !subject.is_empty() {
        head = seq([head, txt(" "), txt(normalize_print_tokens(subject))]);
    }

    let clauses = split_case_clauses(body_tokens)
        .into_iter()
        .map(|clause| txt(normalize_print_tokens(clause)))
        .collect::<Vec<_>>();
    let mut broken = seq([
        head,
        boxed(nest(context.indent_width(), seq([hard(), join(hard, clauses)]))),
        hard(),
        keyword("end", context),
    ]);

    if !tail.is_empty() {
        broken = seq([broken, txt(" "), format_print_tokens_doc(tail.to_vec(), context)]);
    }

    boxed(group(flat_alt(flat, broken)))
}

fn split_case_clauses(tokens: &[PrintToken]) -> Vec<&[PrintToken]> {
    let mut clauses = Vec::new();
    let mut start = None;

    for (index, token) in tokens.iter().enumerate() {
        if (is_keyword_text(token, "when") || is_keyword_text(token, "else"))
            && let Some(previous) = start.replace(index)
        {
            clauses.push(trim_print_tokens(&tokens[previous..index]));
        }
    }

    if let Some(start) = start {
        clauses.push(trim_print_tokens(&tokens[start..]));
    }

    clauses.into_iter().filter(|clause| !clause.is_empty()).collect()
}

fn split_breakable_tokens(
    tokens: &[PrintToken],
    is_break_token: impl Fn(&PrintToken) -> bool,
) -> Option<Vec<&[PrintToken]>> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut parts = Vec::new();
    let mut saw_break = false;

    for (index, token) in tokens.iter().enumerate() {
        if is_open_bracket(token.kind) {
            depth += 1;
            continue;
        }

        if is_close_bracket(token.kind) {
            depth = depth.saturating_sub(1);
            continue;
        }

        if depth == 0 && is_break_token(token) {
            saw_break = true;
            if token.kind == SyntaxKind::Comma {
                parts.push(trim_print_tokens(&tokens[start..index]));
                start = index + 1;
            } else if index > start {
                parts.push(trim_print_tokens(&tokens[start..index]));
                start = index;
            }
        }
    }

    if !saw_break {
        return None;
    }

    parts.push(trim_print_tokens(&tokens[start..]));
    let parts = parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>();
    (parts.len() > 1).then_some(parts)
}

fn flush_print_chunk(docs: &mut Vec<DynDoc>, chunk: &mut Vec<PrintToken>) {
    let trimmed = trim_print_tokens(chunk);
    if !trimmed.is_empty() {
        docs.push(txt(normalize_print_tokens(trimmed)));
    }
    chunk.clear();
}

fn trim_print_tokens(tokens: &[PrintToken]) -> &[PrintToken] {
    let start = tokens
        .iter()
        .position(|token| token.kind != SyntaxKind::Whitespace && !is_layout_token(token.kind))
        .unwrap_or(tokens.len());
    let end = tokens
        .iter()
        .rposition(|token| token.kind != SyntaxKind::Whitespace && !is_layout_token(token.kind))
        .map_or(start, |index| index + 1);
    &tokens[start..end]
}

fn starts_with_keyword(tokens: &[PrintToken], keyword: &str) -> bool {
    tokens.first().is_some_and(|token| is_keyword_text(token, keyword))
}

fn is_keyword_text(token: &PrintToken, keyword: &str) -> bool {
    token.kind == SyntaxKind::Keyword && token.text.eq_ignore_ascii_case(keyword)
}

fn has_token_text(token: &PrintToken, text: &str) -> bool {
    token.text.eq_ignore_ascii_case(text)
}

fn is_comma_break_token(token: &PrintToken) -> bool {
    token.kind == SyntaxKind::Comma
}

fn is_boolean_break_token(token: &PrintToken) -> bool {
    has_token_text(token, "and") || has_token_text(token, "or")
}

fn is_join_break_token(token: &PrintToken) -> bool {
    ["join", "left", "right", "inner", "outer", "full", "cross"]
        .iter()
        .any(|keyword| is_keyword_text(token, keyword))
}

fn bracket_needs_leading_space(token: &PrintToken) -> bool {
    is_keyword_text(token, "as")
        || is_keyword_text(token, "using")
        || matches!(
            token.kind,
            SyntaxKind::BinaryOperator | SyntaxKind::ComparisonOperator | SyntaxKind::Keyword
        )
}

fn token_needs_space_after_bracket_group(token: &PrintToken) -> bool {
    !matches!(
        token.kind,
        SyntaxKind::Comma
            | SyntaxKind::Dot
            | SyntaxKind::StartBracket
            | SyntaxKind::StartSquareBracket
            | SyntaxKind::EndBracket
            | SyntaxKind::EndSquareBracket
            | SyntaxKind::EndCurlyBracket
            | SyntaxKind::Colon
            | SyntaxKind::StatementTerminator
    )
}

fn is_open_bracket(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::StartBracket | SyntaxKind::StartSquareBracket | SyntaxKind::StartCurlyBracket
    )
}

fn is_close_bracket(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EndBracket | SyntaxKind::EndSquareBracket | SyntaxKind::EndCurlyBracket
    )
}

fn matching_close_bracket(kind: SyntaxKind) -> Option<SyntaxKind> {
    match kind {
        SyntaxKind::StartBracket => Some(SyntaxKind::EndBracket),
        SyntaxKind::StartSquareBracket => Some(SyntaxKind::EndSquareBracket),
        SyntaxKind::StartCurlyBracket => Some(SyntaxKind::EndCurlyBracket),
        _ => None,
    }
}

fn find_matching_bracket(tokens: &[PrintToken], open_index: usize) -> Option<usize> {
    let close_kind = matching_close_bracket(tokens.get(open_index)?.kind)?;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open_index) {
        if token.kind == tokens[open_index].kind {
            depth += 1;
        } else if token.kind == close_kind {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

pub(crate) fn node_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.preorder_with_tokens()
        .filter_map(|event| match event {
            WalkEventWithTokens::Token(token) => Some(token),
            WalkEventWithTokens::EnterNode(_) | WalkEventWithTokens::LeaveNode(_) => None,
        })
        .collect()
}

pub(crate) fn is_layout_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Whitespace
            | SyntaxKind::Newline
            | SyntaxKind::Indent
            | SyntaxKind::Dedent
            | SyntaxKind::Implicit
    )
}

pub(crate) fn is_comment(kind: SyntaxKind) -> bool {
    is_comment_kind(kind)
}

pub(crate) fn normalize_print_tokens(tokens: &[PrintToken]) -> String {
    let mut out = String::new();
    let mut previous: Option<PrintToken> = None;

    for (index, token) in tokens.iter().enumerate() {
        if let Some(comment) = token.comment()
            && comment.kind() == CommentKind::Inline
        {
            if !out.is_empty() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
            out.push_str(comment.text());
            out.push('\n');
            previous = Some(token.clone());
            continue;
        }

        if let Some(comment) = token.comment() {
            if !out.is_empty() && !out.ends_with([' ', '\n', '(']) {
                out.push(' ');
            }
            out.push_str(comment.text());
            previous = Some(token.clone());
            continue;
        }

        if needs_space_before(previous.as_ref(), token.kind, &out) {
            out.push(' ');
        }

        out.push_str(&token.text);

        if token.kind == SyntaxKind::Comma
            && tokens.get(index + 1).is_some_and(|next| {
                !matches!(next.kind, SyntaxKind::EndBracket | SyntaxKind::EndSquareBracket)
            })
        {
            out.push(' ');
        }

        previous = Some(token.clone());
    }

    out.trim().to_string()
}

fn format_token_text(token: &SyntaxToken, context: SqlFormatContext<'_>) -> String {
    if token.kind() == SyntaxKind::Keyword
        || matches_keyword_like_operator(token.text(), &["and", "or"])
    {
        context.keyword(token.text())
    } else {
        token.text().to_string()
    }
}

fn matches_keyword_like_operator(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| text.eq_ignore_ascii_case(keyword))
}

fn needs_space_before(previous: Option<&PrintToken>, current: SyntaxKind, out: &str) -> bool {
    if out.is_empty() || out.ends_with([' ', '\n', '(']) {
        return false;
    }

    if current == SyntaxKind::StartBracket {
        return previous.is_some_and(bracket_needs_leading_space);
    }

    if matches!(
        current,
        SyntaxKind::Comma
            | SyntaxKind::Dot
            | SyntaxKind::StartBracket
            | SyntaxKind::StartSquareBracket
            | SyntaxKind::EndBracket
            | SyntaxKind::EndSquareBracket
            | SyntaxKind::EndCurlyBracket
            | SyntaxKind::Colon
            | SyntaxKind::StatementTerminator
    ) {
        return false;
    }

    if matches!(
        previous.map(|token| token.kind),
        Some(
            SyntaxKind::Dot
                | SyntaxKind::StartBracket
                | SyntaxKind::StartSquareBracket
                | SyntaxKind::StartCurlyBracket
                | SyntaxKind::Colon
        )
    ) {
        return false;
    }

    true
}
