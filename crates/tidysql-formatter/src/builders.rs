use tidysql_syntax::SyntaxToken;

use crate::comments::Comment;
use crate::context::SqlFormatContext;
use crate::doc::*;

pub(crate) fn keyword(keyword: &str, context: SqlFormatContext<'_>) -> DynDoc {
    txt(context.keyword(keyword))
}

pub(crate) fn comments_doc(comments: Vec<SyntaxToken>) -> DynDoc {
    let docs = comments
        .into_iter()
        .filter_map(|comment| Comment::classify(&comment))
        .map(|comment| seq([txt(comment.trimmed_text().to_string()), hard()]))
        .collect::<Vec<_>>();
    seq(docs)
}

pub(crate) fn headed_tail(head: DynDoc, tail: DynDoc, indent: usize) -> DynDoc {
    boxed(group(seq([head, boxed(nest(indent, seq([boxed(line()), tail])))])))
}

pub(crate) fn indented_group(head: DynDoc, body: DynDoc, indent: usize) -> DynDoc {
    boxed(group(seq([head, boxed(nest(indent, seq([boxed(line()), body])))])))
}

pub(crate) fn delimited(open: &str, body: DynDoc, close: &str, indent: usize) -> DynDoc {
    boxed(group(seq([txt(open), boxed(nest(indent, seq([soft(), body]))), soft(), txt(close)])))
}
