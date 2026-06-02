#![allow(dead_code)]

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use tidysql_config::{Format, FormatCommaStyle, FormatKeywordCase};
use tidysql_syntax::{
    DialectKind, NodeOrToken, ParseError, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken,
    TextRange, WalkEventWithTokens,
};

#[derive(Debug)]
pub enum FormatError {
    Parse(ParseError),
    UnsupportedSyntax { kind: SyntaxKind, range: TextRange },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Parse(error) => write!(f, "{error}"),
            FormatError::UnsupportedSyntax { kind, range } => write!(
                f,
                "formatting does not yet support {kind:?} at bytes {}..{}",
                usize::from(range.start()),
                usize::from(range.end())
            ),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<ParseError> for FormatError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    Pragmatic,
    Strict,
}

trait Doc {
    fn render(&self, renderer: &mut Render) -> bool;
}

impl<T: Doc + ?Sized> Doc for Box<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

impl<T: Doc + ?Sized> Doc for Rc<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

impl<T: Doc + ?Sized> Doc for Arc<T> {
    fn render(&self, renderer: &mut Render) -> bool {
        self.as_ref().render(renderer)
    }
}

type DynDoc = Box<dyn Doc>;

struct Nil;

impl Doc for Nil {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.nil()
    }
}

struct Text {
    s: String,
}

impl Doc for Text {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.text(&self.s)
    }
}

struct Concat<A, B> {
    a: A,
    b: B,
}

impl<A: Doc, B: Doc> Doc for Concat<A, B> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.concat(|r| self.a.render(r), |r| self.b.render(r))
    }
}

struct Group<D> {
    doc: D,
}

impl<D: Doc> Doc for Group<D> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.group(|r| self.doc.render(r))
    }
}

struct Nest<D> {
    indent: usize,
    doc: D,
}

impl<D: Doc> Doc for Nest<D> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.nest(self.indent, |r| self.doc.render(r))
    }
}

struct FlatAlt<A, B> {
    flat: A,
    broken: B,
}

impl<A: Doc, B: Doc> Doc for FlatAlt<A, B> {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.flat_alt(|r| self.flat.render(r), |r| self.broken.render(r))
    }
}

struct Hardline;

impl Doc for Hardline {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.hardline()
    }
}

struct Fail;

impl Doc for Fail {
    fn render(&self, renderer: &mut Render) -> bool {
        renderer.fail()
    }
}

struct Seq {
    docs: Vec<DynDoc>,
}

impl Doc for Seq {
    fn render(&self, renderer: &mut Render) -> bool {
        for doc in &self.docs {
            if !doc.render(renderer) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flat,
    Broken,
}

#[derive(Clone)]
struct Render {
    width: usize,
    output: String,
    col: usize,
    indent: usize,
    mode: Mode,
}

impl Render {
    fn new(width: usize) -> Self {
        Self { width, output: String::new(), col: 0, indent: 0, mode: Mode::Broken }
    }

    fn render<D: Doc>(&mut self, doc: D) -> String {
        let _ = doc.render(self);
        std::mem::take(&mut self.output)
    }

    fn nil(&mut self) -> bool {
        true
    }

    fn text(&mut self, s: &str) -> bool {
        if self.mode == Mode::Flat && s.contains('\n') {
            return false;
        }

        let width = display_width(s);
        if self.mode == Mode::Flat && self.col + width > self.width {
            return false;
        }

        self.output.push_str(s);
        if s.contains('\n') {
            self.col = width;
        } else {
            self.col += width;
        }
        true
    }

    fn concat(
        &mut self,
        left: impl FnOnce(&mut Render) -> bool,
        right: impl FnOnce(&mut Render) -> bool,
    ) -> bool {
        left(self) && right(self)
    }

    fn group(&mut self, doc: impl Fn(&mut Render) -> bool) -> bool {
        let checkpoint = self.clone();
        let outer_mode = self.mode;
        self.mode = Mode::Flat;
        if doc(self) {
            self.mode = outer_mode;
            return true;
        }

        *self = checkpoint;
        if outer_mode == Mode::Flat {
            return false;
        }

        self.mode = Mode::Broken;
        let rendered = doc(self);
        self.mode = outer_mode;
        rendered
    }

    fn nest(&mut self, indent: usize, doc: impl FnOnce(&mut Render) -> bool) -> bool {
        let previous = self.indent;
        self.indent += indent;
        let rendered = doc(self);
        self.indent = previous;
        rendered
    }

    fn flat_alt(
        &mut self,
        flat: impl FnOnce(&mut Render) -> bool,
        broken: impl FnOnce(&mut Render) -> bool,
    ) -> bool {
        if self.mode == Mode::Flat { flat(self) } else { broken(self) }
    }

    fn hardline(&mut self) -> bool {
        if self.mode == Mode::Flat {
            return false;
        }

        self.output.push('\n');
        for _ in 0..self.indent {
            self.output.push(' ');
        }
        self.col = self.indent;
        true
    }

    fn fail(&mut self) -> bool {
        false
    }
}

fn render<D: Doc>(doc: D, width: usize) -> String {
    Render::new(width).render(doc)
}

fn nil() -> impl Doc {
    Nil
}

fn text(s: impl Into<String>) -> impl Doc {
    Text { s: s.into() }
}

fn concat<A: Doc, B: Doc>(a: A, b: B) -> impl Doc {
    Concat { a, b }
}

fn hardline() -> impl Doc {
    Hardline
}

fn group<D: Doc>(doc: D) -> impl Doc {
    Group { doc }
}

fn nest<D: Doc>(indent: usize, doc: D) -> impl Doc {
    Nest { indent, doc }
}

fn flat_alt<A: Doc, B: Doc>(flat: A, broken: B) -> impl Doc {
    FlatAlt { flat, broken }
}

fn fail() -> impl Doc {
    Fail
}

fn space() -> impl Doc {
    text(" ")
}

fn line() -> impl Doc {
    flat_alt(text(" "), hardline())
}

fn softline() -> impl Doc {
    flat_alt(nil(), hardline())
}

fn soft() -> DynDoc {
    boxed(softline())
}

fn boxed<D: Doc + 'static>(doc: D) -> DynDoc {
    Box::new(doc)
}

fn seq(docs: impl IntoIterator<Item = DynDoc>) -> DynDoc {
    boxed(Seq { docs: docs.into_iter().collect() })
}

fn txt(s: impl Into<String>) -> DynDoc {
    boxed(Text { s: s.into() })
}

fn hard() -> DynDoc {
    boxed(hardline())
}

fn join(separator: impl Fn() -> DynDoc, docs: impl IntoIterator<Item = DynDoc>) -> DynDoc {
    let mut result = Vec::new();
    let mut iter = docs.into_iter();

    let Some(first) = iter.next() else {
        return boxed(nil());
    };

    result.push(first);
    for doc in iter {
        result.push(separator());
        result.push(doc);
    }

    seq(result)
}

fn parens(doc: DynDoc) -> DynDoc {
    seq([txt("("), doc, txt(")")])
}

fn brackets(doc: DynDoc) -> DynDoc {
    seq([txt("["), doc, txt("]")])
}

fn braces(doc: DynDoc) -> DynDoc {
    seq([txt("{"), doc, txt("}")])
}

fn comma_sep(docs: impl IntoIterator<Item = DynDoc>) -> DynDoc {
    join(|| seq([txt(","), txt(" ")]), docs)
}

fn comma_line() -> DynDoc {
    seq([txt(","), boxed(line())])
}

trait DocExt: Doc + Sized {
    fn append<D: Doc>(self, other: D) -> impl Doc {
        concat(self, other)
    }

    fn group(self) -> impl Doc {
        group(self)
    }

    fn nest(self, indent: usize) -> impl Doc {
        nest(indent, self)
    }

    fn boxed(self) -> DynDoc
    where
        Self: 'static,
    {
        boxed(self)
    }
}

impl<T: Doc> DocExt for T {}

pub fn format_with_dialect(source: &str, dialect: DialectKind) -> Result<String, FormatError> {
    format_with_config(source, dialect, &Format::default())
}

pub fn format_with_config(
    source: &str,
    dialect: DialectKind,
    config: &Format,
) -> Result<String, FormatError> {
    format_with_config_and_mode(source, dialect, config, FormatMode::Pragmatic)
}

pub fn format_with_config_strict(
    source: &str,
    dialect: DialectKind,
    config: &Format,
) -> Result<String, FormatError> {
    format_with_config_and_mode(source, dialect, config, FormatMode::Strict)
}

pub fn format_with_config_and_mode(
    source: &str,
    dialect: DialectKind,
    config: &Format,
    mode: FormatMode,
) -> Result<String, FormatError> {
    let tree = tidysql_syntax::parse(source, dialect)?;
    let doc = SqlPrinter::new(config, mode).format_root(&tree.root())?;
    let mut output = render(doc, config.line_width.max(1));

    if source.ends_with('\n') {
        if !output.ends_with('\n') {
            output.push('\n');
        }
    } else {
        while output.ends_with('\n') {
            output.pop();
        }
    }

    Ok(output)
}

struct SqlPrinter<'a> {
    config: &'a Format,
    mode: FormatMode,
}

impl<'a> SqlPrinter<'a> {
    fn new(config: &'a Format, mode: FormatMode) -> Self {
        Self { config, mode }
    }

    fn format_root(&self, root: &SyntaxNode) -> Result<DynDoc, FormatError> {
        if root.text().trim().is_empty() {
            return Ok(txt(root.text()));
        }

        let mut saw_statement = false;
        let mut docs = Vec::new();
        for element in root.children_with_tokens() {
            match element {
                NodeOrToken::Node(node) if node.kind() == SyntaxKind::Statement => {
                    saw_statement = true;
                    docs.push(self.format_statement(&node)?);
                }
                NodeOrToken::Token(token) if token.kind() == SyntaxKind::StatementTerminator => {
                    if let Some(previous) = docs.pop() {
                        docs.push(seq([previous, txt(token.text())]));
                    } else if self.mode == FormatMode::Pragmatic {
                        docs.push(txt(token.text()));
                    } else {
                        return Err(FormatError::UnsupportedSyntax {
                            kind: token.kind(),
                            range: token.text_range(),
                        });
                    }
                }
                NodeOrToken::Token(token)
                    if is_layout_token(token.kind()) || token.kind() == SyntaxKind::EndOfFile => {}
                NodeOrToken::Node(node) if self.mode == FormatMode::Pragmatic => {
                    docs.push(txt(node.text().to_string()));
                }
                NodeOrToken::Token(token) if self.mode == FormatMode::Pragmatic => {
                    docs.push(txt(token.text().to_string()));
                }
                NodeOrToken::Node(node) => return Err(self.unsupported(&node)),
                NodeOrToken::Token(token) => {
                    return Err(FormatError::UnsupportedSyntax {
                        kind: token.kind(),
                        range: token.text_range(),
                    });
                }
            }
        }

        if !saw_statement {
            return match self.mode {
                FormatMode::Pragmatic => Ok(txt(root.text().to_string())),
                FormatMode::Strict => Err(self.unsupported(root)),
            };
        }

        Ok(join(|| txt("\n\n"), docs))
    }

    fn format_statement(&self, statement: &SyntaxNode) -> Result<DynDoc, FormatError> {
        match self.format_statement_strict(statement) {
            Ok(doc) => Ok(doc),
            Err(error @ FormatError::Parse(_)) => Err(error),
            Err(error @ FormatError::UnsupportedSyntax { .. }) => match self.mode {
                FormatMode::Pragmatic => Ok(txt(statement.text().to_string())),
                FormatMode::Strict => Err(error),
            },
        }
    }

    fn format_statement_strict(&self, statement: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let main = statement.children().next().ok_or_else(|| self.unsupported(statement))?;
        let mut doc = match main.kind() {
            SyntaxKind::SelectStatement | SyntaxKind::WithCompoundStatement => {
                self.format_query(&main)?
            }
            _ => return Err(self.unsupported(&main)),
        };

        if statement
            .children_with_tokens()
            .any(|element| matches!(element, NodeOrToken::Token(token) if token.kind() == SyntaxKind::StatementTerminator))
        {
            doc = seq([doc, txt(";")]);
        }

        Ok(doc)
    }

    fn format_query(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        match node.kind() {
            SyntaxKind::SelectStatement => self.format_select_statement(node),
            SyntaxKind::WithCompoundStatement => self.format_with_statement(node),
            _ => Err(self.unsupported(node)),
        }
    }

    fn format_with_statement(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let ctes = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::CommonTableExpression)
            .map(|cte| self.format_cte(&cte))
            .collect::<Result<Vec<_>, _>>()?;
        let select = node
            .children()
            .find(|child| child.kind() == SyntaxKind::SelectStatement)
            .ok_or_else(|| self.unsupported(node))?;

        let cte_count = ctes.len();
        let cte_docs = ctes
            .into_iter()
            .enumerate()
            .map(|(index, cte)| if index + 1 == cte_count { cte } else { seq([cte, txt(",")]) })
            .collect::<Vec<_>>();
        Ok(seq([
            self.leading_comments_doc(node),
            boxed(group(seq([
                kw("with", self.config),
                boxed(nest(
                    self.config.indent_width,
                    seq([boxed(line()), join(|| boxed(line()), cte_docs)]),
                )),
            ]))),
            hard(),
            self.format_select_statement(&select)?,
        ]))
    }

    fn format_cte(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let Some(bracketed) = node.children().find(|child| child.kind() == SyntaxKind::Bracketed)
        else {
            return Ok(txt(self.normalized_node(node)));
        };

        let prefix = node
            .children_with_tokens()
            .take_while(|element| match element {
                NodeOrToken::Node(child) => child.kind() != SyntaxKind::Bracketed,
                NodeOrToken::Token(_) => true,
            })
            .collect::<Vec<_>>();
        let prefix = self.normalized_elements(&prefix);
        let body = bracketed.children().find(|child| {
            matches!(child.kind(), SyntaxKind::SelectStatement | SyntaxKind::WithCompoundStatement)
        });

        let bracketed = if let Some(body) = body {
            seq([
                txt("("),
                boxed(nest(self.config.indent_width, seq([hard(), self.format_query(&body)?]))),
                hard(),
                txt(")"),
            ])
        } else {
            txt(self.normalized_node(&bracketed))
        };

        Ok(seq([txt(prefix), txt(" "), bracketed]))
    }

    fn format_select_statement(&self, node: &SyntaxNode) -> Result<DynDoc, FormatError> {
        let select_clause = node
            .children()
            .find(|child| child.kind() == SyntaxKind::SelectClause)
            .ok_or_else(|| self.unsupported(node))?;
        let mut doc = self.format_select_clause(&select_clause);

        for child in node.children().filter(|child| child.kind() != SyntaxKind::SelectClause) {
            if is_supported_select_clause(child.kind()) {
                doc = seq([doc, hard(), self.format_major_clause(&child)]);
            } else {
                return Err(self.unsupported(&child));
            }
        }

        Ok(doc)
    }

    fn format_select_clause(&self, node: &SyntaxNode) -> DynDoc {
        let elements = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::SelectClauseElement)
            .collect::<Vec<_>>();
        let modifiers = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::SelectClauseModifier)
            .map(|child| self.normalized_node(&child))
            .collect::<Vec<_>>();

        let mut head = seq([self.leading_comments_doc(node), kw("select", self.config)]);
        for modifier in modifiers {
            head = seq([head, txt(" "), txt(modifier)]);
        }

        if elements.is_empty() {
            return self.format_headed_tail(
                head,
                self.tokens_without_head_keyword(node),
                self.config.indent_width,
            );
        }

        let items = elements
            .iter()
            .enumerate()
            .map(|(index, element)| self.format_select_item(index, elements.len(), element))
            .collect::<Vec<_>>();

        let separator = || match self.config.comma_style {
            FormatCommaStyle::Trailing => boxed(line()),
            FormatCommaStyle::Leading => {
                boxed(flat_alt(seq([txt(","), txt(" ")]), seq([hard(), txt(", ")])))
            }
        };
        let list = join(separator, items);

        boxed(group(seq([head, boxed(nest(self.config.indent_width, seq([boxed(line()), list])))])))
    }

    fn format_select_item(&self, index: usize, len: usize, element: &SyntaxNode) -> DynDoc {
        let expr = self.format_expression_like(element);
        match self.config.comma_style {
            FormatCommaStyle::Trailing => {
                if index + 1 == len {
                    expr
                } else {
                    seq([expr, txt(",")])
                }
            }
            FormatCommaStyle::Leading => expr,
        }
    }

    fn format_major_clause(&self, node: &SyntaxNode) -> DynDoc {
        match node.kind() {
            SyntaxKind::FromClause => self.format_clause_with_keyword(node, &["from"]),
            SyntaxKind::WhereClause => self.format_clause_with_keyword(node, &["where"]),
            SyntaxKind::GroupbyClause => self.format_clause_with_keyword(node, &["group", "by"]),
            SyntaxKind::HavingClause => self.format_clause_with_keyword(node, &["having"]),
            SyntaxKind::OrderbyClause => self.format_clause_with_keyword(node, &["order", "by"]),
            SyntaxKind::LimitClause => self.format_clause_with_keyword(node, &["limit"]),
            SyntaxKind::FetchClause => self.format_clause_with_keyword(node, &["fetch"]),
            SyntaxKind::QualifyClause => self.format_clause_with_keyword(node, &["qualify"]),
            SyntaxKind::IntoClause => self.format_clause_with_keyword(node, &["into"]),
            _ => txt(self.normalized_node(node)),
        }
    }

    fn format_clause_with_keyword(&self, node: &SyntaxNode, keywords: &[&str]) -> DynDoc {
        let head = keywords
            .iter()
            .map(|keyword| apply_keyword_case(keyword, self.config.keyword_case))
            .collect::<Vec<_>>()
            .join(" ");
        let tail = self.tokens_without_keyword_prefix(node, keywords.len());
        if tail.is_empty() {
            seq([self.leading_comments_doc(node), txt(head)])
        } else {
            self.format_headed_tail(
                seq([self.leading_comments_doc(node), txt(head)]),
                tail,
                self.config.indent_width,
            )
        }
    }

    fn format_expression_like(&self, node: &SyntaxNode) -> DynDoc {
        self.format_tokens_doc(self.print_tokens(node_tokens(node)))
    }

    fn format_headed_tail(&self, head: DynDoc, tail: Vec<SyntaxToken>, indent: usize) -> DynDoc {
        boxed(group(seq([
            head,
            boxed(nest(
                indent,
                seq([boxed(line()), self.format_tokens_doc(self.print_tokens(tail))]),
            )),
        ])))
    }

    fn leading_comments_doc(&self, node: &SyntaxNode) -> DynDoc {
        comments_doc(
            node.first_token()
                .leading_trivia()
                .filter(|token| is_comment(token.kind()))
                .collect::<Vec<_>>(),
        )
    }

    fn normalized_node_without_head_keyword(&self, node: &SyntaxNode) -> String {
        self.normalized_tokens(self.tokens_without_head_keyword(node))
    }

    fn tokens_without_head_keyword(&self, node: &SyntaxNode) -> Vec<SyntaxToken> {
        node_tokens(node)
            .into_iter()
            .skip_while(|token| token.kind() != SyntaxKind::Keyword)
            .skip(1)
            .collect()
    }

    fn normalized_node_without_keyword_prefix(
        &self,
        node: &SyntaxNode,
        keyword_count: usize,
    ) -> String {
        self.normalized_tokens(self.tokens_without_keyword_prefix(node, keyword_count))
    }

    fn tokens_without_keyword_prefix(
        &self,
        node: &SyntaxNode,
        keyword_count: usize,
    ) -> Vec<SyntaxToken> {
        let mut skipped = 0usize;
        node_tokens(node)
            .into_iter()
            .filter(|token| !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile)
            .filter(|token| {
                if skipped < keyword_count && token.kind() == SyntaxKind::Keyword {
                    skipped += 1;
                    false
                } else {
                    true
                }
            })
            .collect()
    }

    fn normalized_node(&self, node: &SyntaxNode) -> String {
        self.normalized_tokens(node_tokens(node))
    }

    fn normalized_elements(&self, elements: &[SyntaxElement]) -> String {
        let tokens = elements
            .iter()
            .flat_map(|element| match element {
                NodeOrToken::Node(node) => node_tokens(node),
                NodeOrToken::Token(token) => vec![token.clone()],
            })
            .collect::<Vec<_>>();
        self.normalized_tokens(tokens)
    }

    fn normalized_tokens(&self, tokens: Vec<SyntaxToken>) -> String {
        normalize_print_tokens(&self.print_tokens(tokens))
    }

    fn print_tokens(&self, tokens: Vec<SyntaxToken>) -> Vec<PrintToken> {
        let mut pieces = Vec::new();
        for token in tokens {
            for trivia in token.leading_trivia() {
                if is_comment(trivia.kind()) {
                    pieces.push(PrintToken::new(trivia.kind(), trivia.text().to_string()));
                }
            }
            if !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile {
                pieces.push(PrintToken::new(token.kind(), self.format_token_text(&token)));
            }
            for trivia in token.trailing_trivia() {
                if is_comment(trivia.kind()) {
                    pieces.push(PrintToken::new(trivia.kind(), trivia.text().to_string()));
                }
            }
        }
        pieces
    }

    fn format_tokens_doc(&self, tokens: Vec<PrintToken>) -> DynDoc {
        format_print_tokens_doc(tokens, self.config)
    }

    fn format_token_text(&self, token: &SyntaxToken) -> String {
        if token.kind() == SyntaxKind::Keyword
            || matches_keyword_like_operator(token.text(), &["and", "or"])
        {
            apply_keyword_case(token.text(), self.config.keyword_case)
        } else {
            token.text().to_string()
        }
    }

    fn unsupported(&self, node: &SyntaxNode) -> FormatError {
        FormatError::UnsupportedSyntax { kind: node.kind(), range: node.text_range() }
    }
}

#[derive(Clone)]
struct PrintToken {
    kind: SyntaxKind,
    text: String,
}

impl PrintToken {
    fn new(kind: SyntaxKind, text: String) -> Self {
        Self { kind, text }
    }
}

fn format_print_tokens_doc(tokens: Vec<PrintToken>, config: &Format) -> DynDoc {
    let tokens = trim_print_tokens(&tokens);
    if tokens.is_empty() {
        return boxed(nil());
    }

    if tokens.iter().any(|token| is_comment(token.kind)) {
        return txt(normalize_print_tokens(tokens));
    }

    if starts_with_keyword(tokens, "case") {
        return format_case_tokens_doc(tokens, config);
    }

    if let Some(parts) = split_breakable_tokens(tokens, is_boolean_break_token) {
        return boxed(group(join(
            || boxed(line()),
            parts.into_iter().map(|part| format_print_token_range_doc(part, config)),
        )));
    }

    if let Some(parts) = split_breakable_tokens(tokens, is_join_break_token) {
        return boxed(group(join(
            || boxed(line()),
            parts.into_iter().map(|part| format_print_token_range_doc(part, config)),
        )));
    }

    format_print_token_range_doc(tokens, config)
}

fn format_print_token_range_doc(tokens: &[PrintToken], config: &Format) -> DynDoc {
    let mut docs = Vec::new();
    let mut chunk = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if is_open_bracket(token.kind)
            && let Some(close_index) = find_matching_bracket(tokens, index)
        {
            flush_print_chunk(&mut docs, &mut chunk);
            docs.push(format_delimited_tokens_doc(
                &token.text,
                &tokens[index + 1..close_index],
                &tokens[close_index].text,
                config,
            ));
            index = close_index + 1;
            continue;
        }

        chunk.push(token.clone());
        index += 1;
    }

    flush_print_chunk(&mut docs, &mut chunk);
    boxed(group(seq(docs)))
}

fn format_delimited_tokens_doc(
    open: &str,
    inner: &[PrintToken],
    close: &str,
    config: &Format,
) -> DynDoc {
    let inner = trim_print_tokens(inner);
    if inner.is_empty() {
        return seq([txt(open), txt(close)]);
    }

    if let Some(parts) = split_breakable_tokens(inner, is_comma_break_token) {
        let docs = parts
            .into_iter()
            .map(|part| format_print_tokens_doc(part.to_vec(), config))
            .collect::<Vec<_>>();
        return boxed(group(seq([
            txt(open),
            boxed(nest(config.indent_width, seq([soft(), join(comma_line, docs)]))),
            soft(),
            txt(close),
        ])));
    }

    boxed(group(seq([txt(open), format_print_tokens_doc(inner.to_vec(), config), txt(close)])))
}

fn format_case_tokens_doc(tokens: &[PrintToken], config: &Format) -> DynDoc {
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

    let mut head = kw("case", config);
    if !subject.is_empty() {
        head = seq([head, txt(" "), txt(normalize_print_tokens(subject))]);
    }

    let clauses = split_case_clauses(body_tokens)
        .into_iter()
        .map(|clause| txt(normalize_print_tokens(clause)))
        .collect::<Vec<_>>();
    let mut broken = seq([
        head,
        boxed(nest(config.indent_width, seq([hard(), join(hard, clauses)]))),
        hard(),
        kw("end", config),
    ]);

    if !tail.is_empty() {
        broken = seq([broken, txt(" "), format_print_tokens_doc(tail.to_vec(), config)]);
    }

    boxed(group(flat_alt(flat, broken)))
}

fn split_case_clauses(tokens: &[PrintToken]) -> Vec<&[PrintToken]> {
    let mut clauses = Vec::new();
    let mut start = None;

    for (index, token) in tokens.iter().enumerate() {
        if is_keyword_text(token, "when") || is_keyword_text(token, "else") {
            if let Some(previous) = start.replace(index) {
                clauses.push(trim_print_tokens(&tokens[previous..index]));
            }
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

fn kw(keyword: &str, config: &Format) -> DynDoc {
    txt(apply_keyword_case(keyword, config.keyword_case))
}

fn display_width(s: &str) -> usize {
    s.rsplit('\n').next().unwrap_or_default().chars().count()
}

fn apply_keyword_case(keyword: &str, case: FormatKeywordCase) -> String {
    match case {
        FormatKeywordCase::Upper => keyword.to_ascii_uppercase(),
        FormatKeywordCase::Lower => keyword.to_ascii_lowercase(),
        FormatKeywordCase::Preserve => keyword.to_string(),
    }
}

fn matches_keyword_like_operator(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| text.eq_ignore_ascii_case(keyword))
}

fn is_supported_select_clause(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FromClause
            | SyntaxKind::WhereClause
            | SyntaxKind::GroupbyClause
            | SyntaxKind::HavingClause
            | SyntaxKind::OrderbyClause
            | SyntaxKind::LimitClause
            | SyntaxKind::FetchClause
            | SyntaxKind::QualifyClause
            | SyntaxKind::IntoClause
    )
}

fn node_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.preorder_with_tokens()
        .filter_map(|event| match event {
            WalkEventWithTokens::Token(token) => Some(token),
            WalkEventWithTokens::EnterNode(_) | WalkEventWithTokens::LeaveNode(_) => None,
        })
        .collect()
}

fn is_layout_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Whitespace
            | SyntaxKind::Newline
            | SyntaxKind::Indent
            | SyntaxKind::Dedent
            | SyntaxKind::Implicit
    )
}

fn is_comment(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::InlineComment | SyntaxKind::BlockComment | SyntaxKind::Comment)
}

fn comments_doc(comments: Vec<SyntaxToken>) -> DynDoc {
    let docs = comments
        .into_iter()
        .map(|comment| {
            if comment.kind() == SyntaxKind::InlineComment {
                seq([txt(comment.text()), hard()])
            } else {
                seq([txt(comment.text()), hard()])
            }
        })
        .collect::<Vec<_>>();
    seq(docs)
}

fn normalize_print_tokens(tokens: &[PrintToken]) -> String {
    let mut out = String::new();
    let mut previous: Option<SyntaxKind> = None;

    for (index, token) in tokens.iter().enumerate() {
        if token.kind == SyntaxKind::InlineComment {
            if !out.is_empty() && !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
            out.push_str(&token.text);
            out.push('\n');
            previous = Some(token.kind);
            continue;
        }

        if token.kind == SyntaxKind::BlockComment || token.kind == SyntaxKind::Comment {
            if !out.is_empty() && !out.ends_with([' ', '\n', '(']) {
                out.push(' ');
            }
            out.push_str(&token.text);
            previous = Some(token.kind);
            continue;
        }

        if needs_space_before(previous, token.kind, &out) {
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

        previous = Some(token.kind);
    }

    out.trim().to_string()
}

fn needs_space_before(previous: Option<SyntaxKind>, current: SyntaxKind, out: &str) -> bool {
    if out.is_empty() || out.ends_with([' ', '\n', '(']) {
        return false;
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
        previous,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(sql: &str) -> String {
        format_with_dialect(sql, DialectKind::Ansi).unwrap()
    }

    fn fmt_with(sql: &str, config: Format) -> String {
        format_with_config(sql, DialectKind::Ansi, &config).unwrap()
    }

    fn fmt_strict(sql: &str) -> Result<String, FormatError> {
        format_with_config_strict(sql, DialectKind::Ansi, &Format::default())
    }

    fn structure_without_trivia(sql: &str) -> Vec<(SyntaxKind, String)> {
        let tree = tidysql_syntax::parse(sql, DialectKind::Ansi).unwrap();
        node_tokens(&tree.root())
            .into_iter()
            .filter(|token| !is_layout_token(token.kind()) && token.kind() != SyntaxKind::EndOfFile)
            .map(|token| {
                let text =
                    if matches!(token.kind(), SyntaxKind::Keyword | SyntaxKind::BinaryOperator) {
                        token.text().to_ascii_lowercase()
                    } else {
                        token.text().to_string()
                    };
                (token.kind(), text)
            })
            .collect()
    }

    #[test]
    fn doc_group_fits_flat() {
        let doc = group(seq([txt("a"), boxed(line()), txt("b")]));
        assert_eq!(render(doc, 10), "a b");
    }

    #[test]
    fn doc_group_breaks_when_needed() {
        let doc = group(seq([txt("a"), boxed(line()), txt("b")]));
        assert_eq!(render(doc, 1), "a\nb");
    }

    #[test]
    fn doc_nest_indents_after_hardline() {
        let doc = seq([txt("a"), boxed(nest(4, seq([hard(), txt("b")])))]);
        assert_eq!(render(doc, 80), "a\n    b");
    }

    #[test]
    fn doc_flat_alt_uses_flat_branch_in_group() {
        let doc = group(flat_alt(txt(" "), hard()));
        assert_eq!(render(doc, 80), " ");
    }

    #[test]
    fn doc_fail_forces_group_to_break() {
        let doc = group(flat_alt(fail(), txt("broken")));
        assert_eq!(render(doc, 80), "broken");
    }

    #[test]
    fn doc_supports_box_rc_and_arc() {
        let left: Box<dyn Doc> = txt("a");
        let right: Rc<dyn Doc> = Rc::new(Text { s: "b".to_string() });
        let end: Arc<dyn Doc> = Arc::new(Text { s: "c".to_string() });
        assert_eq!(render(seq([left, boxed(right), boxed(end)]), 80), "abc");
    }

    #[test]
    fn simple_select_stays_single_line() {
        assert_eq!(fmt("select a from foo"), "SELECT a\nFROM foo");
    }

    #[test]
    fn formatting_is_idempotent_for_supported_queries() {
        for sql in [
            "select a,b,c from foo where a=1 and b=2",
            "with cte as (select a from foo) select a from cte",
            "select coalesce(long_name, fallback_value, final_value) from foo",
            "select case when a=1 then 'x' else 'y' end as label from t",
        ] {
            let once = fmt(sql);
            let twice = fmt(&once);
            assert_eq!(twice, once, "formatting should be idempotent for {sql}");
        }
    }

    #[test]
    fn formatting_preserves_structure_without_trivia_for_supported_queries() {
        for sql in [
            "select a,b,c from foo where a=1 and b=2",
            "with cte as (select a from foo) select a from cte",
            "select sum(foo), case when a=1 then 'x' else 'y' end as label from t",
        ] {
            let formatted = fmt(sql);
            assert_eq!(
                structure_without_trivia(&formatted),
                structure_without_trivia(sql),
                "formatting should preserve parsed token structure for {sql}",
            );
        }
    }

    #[test]
    fn select_list_uses_trailing_commas() {
        assert_eq!(fmt("select a,b,c from foo"), "SELECT a, b, c\nFROM foo");
    }

    #[test]
    fn select_list_uses_leading_commas() {
        let config = Format { comma_style: FormatCommaStyle::Leading, ..Format::default() };
        assert_eq!(fmt_with("select a,b,c from foo", config), "SELECT a, b, c\nFROM foo");
    }

    #[test]
    fn select_list_breaks_when_line_width_is_narrow() {
        let config = Format { line_width: 12, ..Format::default() };
        assert_eq!(
            fmt_with("select a,b,c from foo", config),
            "SELECT\n    a,\n    b,\n    c\nFROM foo"
        );
    }

    #[test]
    fn leading_commas_apply_when_select_list_breaks() {
        let config =
            Format { comma_style: FormatCommaStyle::Leading, line_width: 12, ..Format::default() };
        assert_eq!(
            fmt_with("select a,b,c from foo", config),
            "SELECT\n    a\n    , b\n    , c\nFROM foo"
        );
    }

    #[test]
    fn keyword_case_can_be_lower() {
        let config = Format { keyword_case: FormatKeywordCase::Lower, ..Format::default() };
        assert_eq!(fmt_with("SELECT a FROM foo", config), "select a\nfrom foo");
    }

    #[test]
    fn keyword_case_can_be_preserved() {
        let config = Format { keyword_case: FormatKeywordCase::Preserve, ..Format::default() };
        assert_eq!(fmt_with("select a from foo", config), "select a\nfrom foo");
    }

    #[test]
    fn formats_cte() {
        assert_eq!(
            fmt("with cte as (select a from foo) select a from cte"),
            "WITH\n    cte AS (\n        SELECT a\n        FROM foo\n    )\nSELECT a\nFROM cte"
        );
    }

    #[test]
    fn formats_join_where_order_limit() {
        assert_eq!(
            fmt("select a from t join u on t.id=u.id where a=1 order by a desc limit 5"),
            "SELECT a\nFROM t JOIN u ON t.id = u.id\nWHERE a = 1\nORDER BY a DESC\nLIMIT 5"
        );
    }

    #[test]
    fn formats_functions_and_case() {
        assert_eq!(
            fmt("select sum(foo), case when a=1 then 'x' else 'y' end as label from t"),
            "SELECT sum(foo), CASE WHEN a = 1 THEN 'x' ELSE 'y' END AS label\nFROM t"
        );
    }

    #[test]
    fn function_arguments_break_when_line_width_is_narrow() {
        let config = Format { line_width: 10, ..Format::default() };
        assert_eq!(
            fmt_with("select coalesce(long_name, fallback_value, final_value) from foo", config),
            "SELECT\n    coalesce(\n        long_name,\n        fallback_value,\n        final_value\n    )\nFROM foo"
        );
    }

    #[test]
    fn boolean_expression_breaks_when_line_width_is_narrow() {
        let config = Format { line_width: 10, ..Format::default() };
        assert_eq!(
            fmt_with("select a from foo where a=1 and b=2 and c=3", config),
            "SELECT a\nFROM foo\nWHERE\n    a = 1\n    AND b = 2\n    AND c = 3"
        );
    }

    #[test]
    fn case_expression_breaks_when_line_width_is_narrow() {
        let config = Format { line_width: 10, ..Format::default() };
        assert_eq!(
            fmt_with(
                "select case when a=1 then 'x' when b=2 then 'y' else 'z' end as label from foo",
                config
            ),
            "SELECT\n    CASE\n        WHEN a = 1 THEN 'x'\n        WHEN b = 2 THEN 'y'\n        ELSE 'z'\n    END AS label\nFROM foo"
        );
    }

    #[test]
    fn joins_break_when_line_width_is_narrow() {
        let config = Format { line_width: 32, ..Format::default() };
        assert_eq!(
            fmt_with(
                "select a from foo join bar on foo.id=bar.foo_id join baz on bar.id=baz.bar_id",
                config
            ),
            "SELECT a\nFROM\n    foo\n    JOIN bar ON foo.id = bar.foo_id\n    JOIN baz ON bar.id = baz.bar_id"
        );
    }

    #[test]
    fn multiple_ctes_keep_commas_in_broken_layout() {
        let config = Format { line_width: 45, ..Format::default() };
        assert_eq!(
            fmt_with(
                "with a as (select x from foo), b as (select y from bar) select x,y from a",
                config
            ),
            "WITH\n    a AS (\n        SELECT x\n        FROM foo\n    ),\n    b AS (\n        SELECT y\n        FROM bar\n    )\nSELECT x, y\nFROM a"
        );
    }

    #[test]
    fn preserves_final_newline() {
        assert_eq!(fmt("select a from foo\n"), "SELECT a\nFROM foo\n");
    }

    #[test]
    fn parse_errors_are_returned() {
        let error = format_with_dialect("select from foo", DialectKind::Ansi).unwrap_err();
        assert!(matches!(
            error,
            FormatError::Parse(ParseError::UnparsableRanges(_) | ParseError::Parse(_))
        ));
    }

    #[test]
    fn unsupported_statement_is_preserved_by_default() {
        assert_eq!(fmt("insert into foo values (1)"), "insert into foo values (1)");
    }

    #[test]
    fn unsupported_statement_errors_in_strict_mode() {
        let error = fmt_strict("insert into foo values (1)").unwrap_err();
        assert!(matches!(
            error,
            FormatError::UnsupportedSyntax { kind: SyntaxKind::InsertStatement, .. }
        ));
    }

    #[test]
    fn mixed_supported_and_unsupported_statements_format_pragmatically() {
        assert_eq!(
            fmt("select a from foo; insert into foo values (1); select b from bar;"),
            "SELECT a\nFROM foo;\n\ninsert into foo values (1);\n\nSELECT b\nFROM bar;"
        );
    }
}
