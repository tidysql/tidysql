use std::fmt;
use std::ops::Range;
use std::rc::Rc;

use get_size2::GetSize;
use sqruff_lib_dialects::kind_to_dialect;
pub use sqruff_parser_core::dialects::{DialectKind, SyntaxKind, SyntaxSet};
use sqruff_parser_core::errors::{SQLLexError, SQLParseError};
use sqruff_parser_core::parser::Parser;
use sqruff_parser_core::parser::event_sink::EventSink;
use sqruff_parser_core::parser::lexer::Lexer;
use sqruff_parser_core::parser::token::Token as ParserToken;
pub use text_size::{TextLen, TextRange, TextSize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, GetSize)]
pub enum NodeOrToken<N, T> {
    Node(N),
    Token(T),
}

impl<N: Copy, T: Copy> Copy for NodeOrToken<N, T> {}

impl<N, T> NodeOrToken<N, T> {
    pub fn into_node(self) -> Option<N> {
        match self {
            NodeOrToken::Node(node) => Some(node),
            NodeOrToken::Token(_) => None,
        }
    }

    pub fn into_token(self) -> Option<T> {
        match self {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(token) => Some(token),
        }
    }

    pub fn as_node(&self) -> Option<&N> {
        match self {
            NodeOrToken::Node(node) => Some(node),
            NodeOrToken::Token(_) => None,
        }
    }

    pub fn as_token(&self) -> Option<&T> {
        match self {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(token) => Some(token),
        }
    }
}

impl<N: fmt::Display, T: fmt::Display> fmt::Display for NodeOrToken<N, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeOrToken::Node(node) => fmt::Display::fmt(node, f),
            NodeOrToken::Token(token) => fmt::Display::fmt(token, f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, GetSize)]
pub(crate) struct NodeId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, GetSize)]
pub struct TokenId(pub(crate) usize);

pub(crate) type NodeOrTokenRef = NodeOrToken<NodeId, TokenId>;

#[derive(Clone, Copy, GetSize)]
pub(crate) struct Token {
    #[get_size(ignore)]
    pub(crate) kind: SyntaxKind,
    pub(crate) attached_trivia: AttachedTrivia,
    #[get_size(ignore)]
    pub(crate) end: TextSize,
    pub(crate) parent: NodeId,
}

#[derive(Clone, Copy, GetSize)]
pub(crate) struct AttachedTrivia {
    has_leading_trivia: bool,
    has_trailing_trivia: bool,
    trivia_len: u16,
}

impl AttachedTrivia {
    #[inline]
    pub(crate) fn new(
        has_leading_trivia: bool,
        has_trailing_trivia: bool,
        trivia_len: usize,
    ) -> AttachedTrivia {
        AttachedTrivia {
            has_leading_trivia,
            has_trailing_trivia,
            trivia_len: u16::try_from(trivia_len).expect("trivia_len must fit into u16"),
        }
    }

    #[inline]
    pub(crate) fn has_leading_trivia(self) -> bool {
        self.has_leading_trivia
    }

    #[inline]
    pub(crate) fn has_trailing_trivia(self) -> bool {
        self.has_trailing_trivia
    }

    #[inline]
    pub(crate) fn trivia_len(self) -> usize {
        self.trivia_len as usize
    }
}

impl TokenId {
    #[inline]
    pub(crate) fn get(self, tree: &TreeInner) -> &Token {
        &tree.tokens[self.0]
    }

    #[inline]
    fn prev_maybe_fake_token(self, tree: &TreeInner) -> &Token {
        &tree.tokens[self.0 - 1]
    }

    #[inline]
    pub(crate) fn start(self, tree: &TreeInner) -> TextSize {
        self.prev_maybe_fake_token(tree).end
    }

    #[inline]
    pub(crate) fn end(self, tree: &TreeInner) -> TextSize {
        self.get(tree).end
    }

    #[inline]
    pub(crate) fn text_range(self, tree: &TreeInner) -> TextRange {
        TextRange::new(self.start(tree), self.get(tree).end)
    }

    #[inline]
    pub(crate) fn text(self, tree: &TreeInner) -> &str {
        &tree.text[self.text_range(tree)]
    }

    #[inline]
    pub(crate) fn prev_token(self) -> Option<Self> {
        if self.0 <= 1 { None } else { Some(TokenId(self.0 - 1)) }
    }

    #[inline]
    pub(crate) fn next_token(self, tree: &TreeInner) -> Option<Self> {
        let next = self.0 + 1;
        if next >= tree.tokens.len() { None } else { Some(TokenId(next)) }
    }

    #[inline]
    pub(crate) fn leading_trivia(self, tree: &TreeInner) -> TokenIter {
        if !self.get(tree).attached_trivia.has_leading_trivia() {
            return TokenIter::empty();
        }

        let trivia_len = self.get(tree).attached_trivia.trivia_len();
        let trivia_start = self.0 - trivia_len;
        TokenIter::new(trivia_start, trivia_len)
    }

    #[inline]
    pub(crate) fn trailing_trivia(self, tree: &TreeInner) -> TokenIter {
        if !self.get(tree).attached_trivia.has_trailing_trivia() {
            return TokenIter::empty();
        }

        let trivia_start = self.0 + 1;
        let trivia_len = tree.tokens[trivia_start].attached_trivia.trivia_len();
        TokenIter::new(trivia_start, trivia_len)
    }

    #[inline]
    pub(crate) fn parent(self, tree: &TreeInner) -> NodeId {
        self.get(tree).parent
    }
}

#[derive(Clone)]
pub(crate) struct TokenIter {
    current: usize,
    end: usize,
}

impl TokenIter {
    #[inline]
    fn new(start: usize, len: usize) -> Self {
        Self { current: start, end: start + len }
    }

    #[inline]
    fn empty() -> Self {
        Self { current: 0, end: 0 }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.end - self.current
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.current == self.end
    }
}

impl Iterator for TokenIter {
    type Item = TokenId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.is_empty() {
            None
        } else {
            let result = TokenId(self.current);
            self.current += 1;
            Some(result)
        }
    }
}

impl DoubleEndedIterator for TokenIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.is_empty() {
            None
        } else {
            self.end -= 1;
            Some(TokenId(self.end))
        }
    }
}

#[derive(GetSize)]
pub(crate) struct Nodes {
    pub(crate) nodes: Vec<Node>,
    pub(crate) node_children: Vec<NodeOrTokenRef>,
}

impl Nodes {}

#[derive(GetSize)]
pub(crate) struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Range<usize>,
    #[get_size(ignore)]
    pub(crate) kind: SyntaxKind,
    pub(crate) first_token: TokenId,
    pub(crate) last_token: TokenId,
}

impl Node {
    #[inline]
    pub(crate) fn text_range(&self, tree: &TreeInner) -> TextRange {
        TextRange::new(self.first_token(tree).start(tree), self.last_token(tree).end(tree))
    }

    #[inline]
    pub(crate) fn text<'a>(&self, tree: &'a TreeInner) -> &'a str {
        let range = self.text_range(tree);
        &tree.text[range]
    }

    #[inline]
    pub(crate) fn first_token(&self, _tree: &TreeInner) -> TokenId {
        self.first_token
    }

    #[inline]
    pub(crate) fn last_token(&self, _tree: &TreeInner) -> TokenId {
        self.last_token
    }

    #[inline]
    pub(crate) fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    #[inline]
    pub(crate) fn children<'a>(&self, tree: &'a TreeInner) -> &'a [NodeOrTokenRef] {
        &tree.nodes.node_children[self.children.clone()]
    }

    #[inline]
    fn tokens_range<'a>(&self, tree: &'a TreeInner) -> &'a [Token] {
        let start = self.first_token.0;
        let end = self.last_token.0.saturating_add(1);
        &tree.tokens[start..end]
    }

    #[inline]
    pub(crate) fn token_at_offset(
        &self,
        tree: &TreeInner,
        offset: TextSize,
    ) -> TokenAtOffset<TokenId> {
        let tokens_range = self.tokens_range(tree);
        let index = tokens_range.partition_point(|token| token.end <= offset);
        let token_index = self.first_token.0 + index;
        if token_index >= tree.tokens.len() {
            return TokenAtOffset::None;
        }
        let second_token = TokenId(token_index);
        if second_token.end(tree) <= offset {
            return TokenAtOffset::None;
        }
        if let Some(first_token) = second_token.prev_token()
            && first_token.end(tree) == offset
        {
            TokenAtOffset::Between(first_token, second_token)
        } else {
            TokenAtOffset::Single(second_token)
        }
    }

    #[inline]
    pub(crate) fn covering_element(&self, tree: &TreeInner, range: TextRange) -> NodeOrTokenRef {
        let token = self
            .token_at_offset(tree, range.start())
            .right_biased()
            .expect("range is not inside the node");
        if token.text_range(tree).contains_range(range) {
            return NodeOrTokenRef::Token(token);
        }
        let mut current = token.parent(tree);
        loop {
            let node = &tree.nodes.nodes[current.0];
            if node.text_range(tree).contains_range(range) {
                return NodeOrTokenRef::Node(current);
            }
            current = node.parent.expect("range is not inside the node");
        }
    }
}

#[derive(Clone, GetSize)]
pub(crate) struct Tree(pub(crate) Rc<TreeInner>);

#[derive(GetSize)]
pub(crate) struct TreeInner {
    pub(crate) text: String,
    pub(crate) tokens: Vec<Token>,
    pub(crate) nodes: Nodes,
}

#[derive(GetSize)]
pub struct SyntaxTree {
    pub(crate) tree: Tree,
}

impl Clone for SyntaxTree {
    #[inline]
    fn clone(&self) -> Self {
        Self { tree: self.tree.clone() }
    }
}

impl SyntaxTree {
    #[inline]
    pub fn root(&self) -> SyntaxNode {
        SyntaxNode { tree: self.tree.clone(), node: NodeId(0) }
    }

    #[inline]
    pub fn text(&self) -> &str {
        &self.tree.0.text
    }

    #[inline]
    pub fn token_text(&self, token: TokenId) -> &str {
        token.text(&self.tree.0)
    }
}

#[derive(Clone)]
pub struct SyntaxToken {
    tree: Tree,
    token: TokenId,
}

impl SyntaxToken {
    #[inline]
    pub fn id(&self) -> TokenId {
        self.token
    }

    #[inline]
    pub fn kind(&self) -> SyntaxKind {
        self.token.get(&self.tree.0).kind
    }

    #[inline]
    pub fn text_range(&self) -> TextRange {
        self.token.text_range(&self.tree.0)
    }

    #[inline]
    pub fn prev_token(&self) -> Option<Self> {
        Some(Self { tree: self.tree.clone(), token: self.token.prev_token()? })
    }

    #[inline]
    pub fn next_token(&self) -> Option<Self> {
        Some(Self { tree: self.tree.clone(), token: self.token.next_token(&self.tree.0)? })
    }

    #[inline]
    pub fn leading_trivia(&self) -> TriviaIter {
        TriviaIter { tree: self.tree.clone(), tokens: self.token.leading_trivia(&self.tree.0) }
    }

    #[inline]
    pub fn trailing_trivia(&self) -> TriviaIter {
        TriviaIter { tree: self.tree.clone(), tokens: self.token.trailing_trivia(&self.tree.0) }
    }

    #[inline]
    pub fn text(&self) -> &str {
        self.token.text(&self.tree.0)
    }

    #[inline]
    pub fn text_range_including_trivia(&self) -> TextRange {
        let first_token = self.leading_trivia().next().unwrap_or_else(|| self.clone());
        let last_token = self.trailing_trivia().next_back().unwrap_or_else(|| self.clone());
        let tree = &self.tree.0;
        TextRange::new(first_token.token.start(tree), last_token.token.end(tree))
    }

    #[inline]
    pub fn text_including_trivia(&self) -> &str {
        &self.tree.0.text[self.text_range_including_trivia()]
    }

    #[inline]
    pub fn parent(&self) -> SyntaxNode {
        SyntaxNode { tree: self.tree.clone(), node: self.token.parent(&self.tree.0) }
    }

    #[inline]
    pub fn parent_ancestors(&self) -> impl Iterator<Item = SyntaxNode> + Clone {
        std::iter::successors(Some(self.parent()), |it: &SyntaxNode| it.parent())
    }
}

#[derive(Clone)]
pub struct TriviaIter {
    tree: Tree,
    tokens: TokenIter,
}

impl Iterator for TriviaIter {
    type Item = SyntaxToken;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(SyntaxToken { tree: self.tree.clone(), token: self.tokens.next()? })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.tokens.len();
        (len, Some(len))
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.next_back()
    }
}

impl DoubleEndedIterator for TriviaIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(SyntaxToken { tree: self.tree.clone(), token: self.tokens.next_back()? })
    }
}

impl ExactSizeIterator for TriviaIter {
    #[inline]
    fn len(&self) -> usize {
        self.tokens.len()
    }
}

#[derive(Clone, Debug)]
pub enum TokenAtOffset<T = SyntaxToken> {
    None,
    Single(T),
    Between(T, T),
}

impl<T: Copy> Copy for TokenAtOffset<T> {}

impl<T> TokenAtOffset<T> {
    pub fn right_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) => Some(token),
            TokenAtOffset::Between(_, right) => Some(right),
        }
    }

    pub fn left_biased(self) -> Option<T> {
        match self {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) => Some(token),
            TokenAtOffset::Between(left, _) => Some(left),
        }
    }
}

impl<T> Iterator for TokenAtOffset<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match std::mem::replace(self, TokenAtOffset::None) {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) => {
                *self = TokenAtOffset::None;
                Some(token)
            }
            TokenAtOffset::Between(left, right) => {
                *self = TokenAtOffset::Single(right);
                Some(left)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            TokenAtOffset::None => (0, Some(0)),
            TokenAtOffset::Single(_) => (1, Some(1)),
            TokenAtOffset::Between(_, _) => (2, Some(2)),
        }
    }
}

impl<T> ExactSizeIterator for TokenAtOffset<T> {}

pub type SyntaxElement = NodeOrToken<SyntaxNode, SyntaxToken>;

#[derive(Clone)]
pub struct SyntaxNode {
    tree: Tree,
    node: NodeId,
}

impl SyntaxNode {
    #[inline]
    fn node_data(&self) -> &Node {
        &self.tree.0.nodes.nodes[self.node.0]
    }

    #[inline]
    pub(crate) fn same_tree(&self, other: &SyntaxNode) -> bool {
        std::rc::Rc::ptr_eq(&self.tree.0, &other.tree.0)
    }

    #[inline]
    pub fn kind(&self) -> SyntaxKind {
        self.node_data().kind
    }

    #[inline]
    pub fn first_token(&self) -> SyntaxToken {
        SyntaxToken { tree: self.tree.clone(), token: self.node_data().first_token(&self.tree.0) }
    }

    #[inline]
    pub fn last_token(&self) -> SyntaxToken {
        SyntaxToken { tree: self.tree.clone(), token: self.node_data().last_token(&self.tree.0) }
    }

    #[inline]
    pub fn text_range(&self) -> TextRange {
        self.node_data().text_range(&self.tree.0)
    }

    #[inline]
    pub fn text(&self) -> &str {
        self.node_data().text(&self.tree.0)
    }

    #[inline]
    pub fn parent(&self) -> Option<Self> {
        Some(Self { tree: self.tree.clone(), node: self.node_data().parent()? })
    }

    #[inline]
    pub fn try_token_at(&self, index: usize) -> Option<SyntaxToken> {
        match *self.node_data().children(&self.tree.0).get(index)? {
            NodeOrTokenRef::Token(token) => Some(SyntaxToken { tree: self.tree.clone(), token }),
            NodeOrTokenRef::Node(_) => None,
        }
    }

    #[inline]
    #[track_caller]
    pub fn token_at(&self, index: usize) -> SyntaxToken {
        match self.try_token_at(index) {
            Some(it) => it,
            None => expected_token(index),
        }
    }

    #[inline]
    pub fn try_node_at(&self, index: usize) -> Option<SyntaxNode> {
        match *self.node_data().children(&self.tree.0).get(index)? {
            NodeOrTokenRef::Node(node) => Some(SyntaxNode { tree: self.tree.clone(), node }),
            NodeOrTokenRef::Token(_) => None,
        }
    }

    #[inline]
    #[track_caller]
    pub fn node_at(&self, index: usize) -> SyntaxNode {
        match self.try_node_at(index) {
            Some(it) => it,
            None => expected_node(index),
        }
    }

    #[inline]
    pub fn try_child_at(&self, index: usize) -> Option<SyntaxElement> {
        self.node_data()
            .children(&self.tree.0)
            .get(index)
            .copied()
            .map(|child| map_node_or_token_ref(&self.tree, child))
    }

    #[inline]
    #[track_caller]
    pub fn child_at(&self, index: usize) -> SyntaxElement {
        map_node_or_token_ref(&self.tree, self.node_data().children(&self.tree.0)[index])
    }

    #[inline]
    pub fn ancestors(&self) -> impl Iterator<Item = Self> + Clone {
        std::iter::successors(Some(self.clone()), |it| it.parent())
    }

    #[inline]
    pub fn children_with_tokens(&self) -> ChildrenWithTokens {
        ChildrenWithTokens { tree: self.tree.clone(), range: self.node_data().children.clone() }
    }

    #[inline]
    pub fn children(&self) -> Children {
        Children { inner: self.children_with_tokens() }
    }

    #[inline]
    pub fn preorder(&self) -> Preorder {
        Preorder::new(self.clone())
    }

    #[inline]
    pub fn preorder_with_tokens(&self) -> PreorderWithTokens {
        PreorderWithTokens::new(self.clone())
    }

    #[inline]
    pub fn descendants(&self) -> impl Iterator<Item = Self> + Clone {
        self.preorder_with_tokens().filter_map(|event| match event {
            WalkEventWithTokens::EnterNode(node) => Some(node),
            WalkEventWithTokens::LeaveNode(_) | WalkEventWithTokens::Token(_) => None,
        })
    }

    #[inline]
    pub fn descendants_with_tokens(&self) -> impl Iterator<Item = SyntaxElement> + Clone {
        self.preorder_with_tokens().filter_map(|event| match event {
            WalkEventWithTokens::EnterNode(node) => Some(SyntaxElement::Node(node)),
            WalkEventWithTokens::Token(node) => Some(SyntaxElement::Token(node)),
            WalkEventWithTokens::LeaveNode(_) => None,
        })
    }

    #[inline]
    pub fn token_at_offset(&self, offset: TextSize) -> TokenAtOffset {
        match self.node_data().token_at_offset(&self.tree.0, offset) {
            TokenAtOffset::None => TokenAtOffset::None,
            TokenAtOffset::Single(token) => {
                TokenAtOffset::Single(SyntaxToken { tree: self.tree.clone(), token })
            }
            TokenAtOffset::Between(left, right) => TokenAtOffset::Between(
                SyntaxToken { tree: self.tree.clone(), token: left },
                SyntaxToken { tree: self.tree.clone(), token: right },
            ),
        }
    }

    #[inline]
    pub fn covering_element(&self, range: TextRange) -> SyntaxElement {
        map_node_or_token_ref(&self.tree, self.node_data().covering_element(&self.tree.0, range))
    }
}

#[inline]
fn map_node_or_token_ref(tree: &Tree, child: NodeOrTokenRef) -> SyntaxElement {
    match child {
        NodeOrTokenRef::Token(token) => {
            SyntaxElement::Token(SyntaxToken { tree: tree.clone(), token })
        }
        NodeOrTokenRef::Node(node) => SyntaxElement::Node(SyntaxNode { tree: tree.clone(), node }),
    }
}

#[cold]
#[inline(never)]
#[track_caller]
fn expected_token(idx: usize) -> ! {
    panic!("expected a token at index {idx}")
}

#[cold]
#[inline(never)]
#[track_caller]
fn expected_node(idx: usize) -> ! {
    panic!("expected a node at index {idx}")
}

#[derive(Clone)]
pub struct ChildrenWithTokens {
    tree: Tree,
    range: std::ops::Range<usize>,
}

impl ChildrenWithTokens {
    #[inline]
    fn map_index(&self, index: usize) -> SyntaxElement {
        map_node_or_token_ref(&self.tree, self.tree.0.nodes.node_children[index])
    }
}

impl Iterator for ChildrenWithTokens {
    type Item = SyntaxElement;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().map(|index| self.map_index(index))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.next_back()
    }
}

impl DoubleEndedIterator for ChildrenWithTokens {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back().map(|index| self.map_index(index))
    }
}

impl ExactSizeIterator for ChildrenWithTokens {
    #[inline]
    fn len(&self) -> usize {
        self.range.len()
    }
}

#[derive(Clone)]
pub struct Children {
    inner: ChildrenWithTokens,
}

impl Children {
    #[inline]
    fn filter_child(child: SyntaxElement) -> Option<SyntaxNode> {
        match child {
            SyntaxElement::Node(it) => Some(it),
            SyntaxElement::Token(_) => None,
        }
    }

    #[inline]
    fn iter(self) -> impl Iterator<Item = SyntaxNode> {
        self.inner.filter_map(|child| match child {
            SyntaxElement::Node(it) => Some(it),
            SyntaxElement::Token(_) => None,
        })
    }
}

impl Iterator for Children {
    type Item = SyntaxNode;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.find_map(Self::filter_child)
    }

    #[inline]
    fn fold<B, F>(self, init: B, f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter().fold(init, f)
    }

    #[inline]
    fn for_each<F>(self, f: F)
    where
        Self: Sized,
        F: FnMut(Self::Item),
    {
        self.iter().for_each(f);
    }
}

#[derive(Clone)]
pub struct Preorder {
    inner: PreorderWithTokens,
}

impl Preorder {
    #[inline]
    fn new(start: SyntaxNode) -> Preorder {
        Preorder { inner: PreorderWithTokens::new(start) }
    }

    #[inline]
    pub fn skip_subtree(&mut self) {
        self.inner.skip_subtree();
    }
}

impl Iterator for Preorder {
    type Item = WalkEvent;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.find_map(|item| match item {
            WalkEventWithTokens::EnterNode(it) => Some(WalkEvent::Enter(it)),
            WalkEventWithTokens::LeaveNode(it) => Some(WalkEvent::Leave(it)),
            WalkEventWithTokens::Token(_) => None,
        })
    }
}

#[derive(Clone)]
pub enum WalkEvent {
    Enter(SyntaxNode),
    Leave(SyntaxNode),
}

#[derive(Clone)]
pub struct PreorderWithTokens {
    stack: Vec<(SyntaxNode, ChildrenWithTokens)>,
    root: Option<SyntaxNode>,
}

impl PreorderWithTokens {
    #[inline]
    fn new(start: SyntaxNode) -> PreorderWithTokens {
        PreorderWithTokens { stack: Vec::with_capacity(128), root: Some(start) }
    }

    #[inline]
    pub fn skip_subtree(&mut self) {
        assert!(self.stack.pop().is_some(), "must have a subtree to skip");
    }
}

impl Iterator for PreorderWithTokens {
    type Item = WalkEventWithTokens;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(root) = self.root.take() {
            self.stack.push((root.clone(), root.children_with_tokens()));
            return Some(WalkEventWithTokens::EnterNode(root));
        }
        let (_, active_node) = self.stack.last_mut()?;
        match active_node.next() {
            Some(SyntaxElement::Node(child)) => {
                self.stack.push((child.clone(), child.children_with_tokens()));
                Some(WalkEventWithTokens::EnterNode(child))
            }
            Some(SyntaxElement::Token(child)) => Some(WalkEventWithTokens::Token(child)),
            None => {
                let (exited_node, _) = self.stack.pop().expect("should have an exited-from node");
                Some(WalkEventWithTokens::LeaveNode(exited_node))
            }
        }
    }
}

#[derive(Clone)]
pub enum WalkEventWithTokens {
    EnterNode(SyntaxNode),
    LeaveNode(SyntaxNode),
    Token(SyntaxToken),
}

impl PartialEq for SyntaxNode {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.same_tree(other)
    }
}

impl Eq for SyntaxNode {}

impl std::hash::Hash for SyntaxNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tree_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        tree_ptr.hash(state);
        self.node.hash(state);
    }
}

impl PartialOrd for SyntaxNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SyntaxNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        let other_ptr = std::rc::Rc::as_ptr(&other.tree.0) as usize;
        self_ptr.cmp(&other_ptr).then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialEq for SyntaxToken {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token && std::rc::Rc::ptr_eq(&self.tree.0, &other.tree.0)
    }
}

impl Eq for SyntaxToken {}

impl std::hash::Hash for SyntaxToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tree_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        tree_ptr.hash(state);
        self.token.hash(state);
    }
}

impl PartialOrd for SyntaxToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SyntaxToken {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        let other_ptr = std::rc::Rc::as_ptr(&other.tree.0) as usize;
        self_ptr.cmp(&other_ptr).then_with(|| self.token.cmp(&other.token))
    }
}

impl fmt::Debug for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let mut level = 0;
            for event in self.preorder_with_tokens() {
                match event {
                    WalkEventWithTokens::EnterNode(node) => {
                        for _ in 0..level {
                            write!(f, "  ")?;
                        }
                        writeln!(f, "{:?}", node)?;
                        level += 1;
                    }
                    WalkEventWithTokens::Token(token) => {
                        for _ in 0..level {
                            write!(f, "  ")?;
                        }
                        writeln!(f, "{:?}", token)?;
                    }
                    WalkEventWithTokens::LeaveNode(_) => level -= 1,
                }
            }
            assert_eq!(level, 0);
            Ok(())
        } else {
            write!(f, "{:?}@{:?}", self.kind(), self.text_range())
        }
    }
}

impl fmt::Display for SyntaxNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.text(), f)
    }
}

impl fmt::Debug for SyntaxToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}@{:?}", self.kind(), self.text_range())?;
        if self.text().len() < 25 {
            return write!(f, " {:?}", self.text());
        }
        let text = self.text();
        for idx in 21..25 {
            if text.is_char_boundary(idx) {
                let text = format!("{} ...", &text[..idx]);
                return write!(f, " {:?}", text);
            }
        }
        unreachable!()
    }
}

impl fmt::Display for SyntaxToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.text(), f)
    }
}

impl From<SyntaxNode> for SyntaxElement {
    #[inline]
    fn from(node: SyntaxNode) -> SyntaxElement {
        NodeOrToken::Node(node)
    }
}

impl From<SyntaxToken> for SyntaxElement {
    #[inline]
    fn from(token: SyntaxToken) -> SyntaxElement {
        NodeOrToken::Token(token)
    }
}

struct Frame {
    id: NodeId,
    children: Vec<NodeOrTokenRef>,
    token_range: Option<(TokenId, TokenId)>,
}

struct VecPool<T> {
    pool: Vec<Vec<T>>,
    default_cap: usize,
}

struct PendingToken {
    kind: SyntaxKind,
    text_len: TextSize,
}

#[derive(Default)]
struct TriviaState {
    pending: Option<PendingToken>,
    leading: Vec<(SyntaxKind, TextSize)>,
    trailing: Vec<(SyntaxKind, TextSize)>,
}

impl TriviaState {
    fn new() -> Self {
        Self { pending: None, leading: Vec::with_capacity(8), trailing: Vec::with_capacity(8) }
    }

    fn abandon(&mut self) {
        self.pending = None;
        self.leading.clear();
        self.trailing.clear();
    }

    fn flush_into(&mut self, out: &mut TreeBuilder) {
        let Some(pending) = self.pending.take() else {
            return;
        };

        let mut leading = std::mem::take(&mut self.leading);
        let mut trailing = std::mem::take(&mut self.trailing);

        out.emit_token_with_trivia(
            leading.drain(..),
            pending.kind,
            pending.text_len,
            trailing.drain(..),
        );

        self.leading = leading;
        self.trailing = trailing;
    }

    fn push_trivia(&mut self, token: &ParserToken) {
        let text_len = TextSize::of(token.raw.as_str());
        if self.pending.is_some() {
            self.trailing.push((token.kind, text_len));
        } else {
            self.leading.push((token.kind, text_len));
        }
    }

    fn emit_meta(&mut self, out: &mut TreeBuilder, token: &ParserToken) {
        let text_len = TextSize::of(token.raw.as_str());
        let mut leading = std::mem::take(&mut self.leading);

        out.emit_token_with_trivia(leading.drain(..), token.kind, text_len, std::iter::empty());

        self.leading = leading;
    }

    fn set_pending(&mut self, token: &ParserToken) {
        self.pending =
            Some(PendingToken { kind: token.kind, text_len: TextSize::of(token.raw.as_str()) });
    }

    fn on_token(&mut self, out: &mut TreeBuilder, token: &ParserToken) {
        if token.is_whitespace() || token.is_comment() {
            self.push_trivia(token);
            return;
        }

        if token.is_meta() {
            self.flush_into(out);
            self.emit_meta(out, token);
            return;
        }

        self.flush_into(out);
        self.set_pending(token);
    }
}

impl<T> VecPool<T> {
    fn new(pool_cap: usize, default_cap: usize) -> Self {
        Self { pool: Vec::with_capacity(pool_cap), default_cap }
    }

    fn take(&mut self) -> Vec<T> {
        self.pool.pop().unwrap_or_else(|| Vec::with_capacity(self.default_cap))
    }

    fn give(&mut self, mut vec: Vec<T>) {
        vec.clear();
        self.pool.push(vec);
    }
}

pub struct TreeBuilder {
    nodes: Vec<Node>,
    node_children: Vec<NodeOrTokenRef>,
    tokens: Vec<Token>,
    text: String,

    node_children_pool: VecPool<NodeOrTokenRef>,
    opened: Vec<Frame>,
    text_cursor: TextSize,

    trivia: TriviaState,
}

impl Drop for TreeBuilder {
    fn drop(&mut self) {
        if !std::thread::panicking() && !self.opened.is_empty() {
            panic!("you should call `TreeBuilder::finish()`");
        }
    }
}

const DEFAULT_TREE_DEPTH: usize = 128;
const DEFAULT_TREE_SIZE: usize = 1024;
const DEFAULT_CHILDREN_LEN: usize = 10;

impl TreeBuilder {
    pub(crate) fn new_rootless_with_caps(source: impl Into<String>, token_cap: usize) -> Self {
        Self::new_impl(source.into(), None, token_cap)
    }

    fn new_impl(text: String, root_kind: Option<SyntaxKind>, token_cap: usize) -> Self {
        let mut nodes = Vec::with_capacity(DEFAULT_TREE_SIZE);
        let mut node_children_pool = VecPool::new(DEFAULT_TREE_DEPTH, DEFAULT_CHILDREN_LEN);
        let mut opened = Vec::with_capacity(DEFAULT_TREE_DEPTH);
        if let Some(kind) = root_kind {
            nodes.push(Node {
                parent: None,
                kind,
                children: 0..0,
                first_token: TokenId(0),
                last_token: TokenId(0),
            });
            opened.push(Frame {
                id: NodeId(0),
                children: node_children_pool.take(),
                token_range: None,
            });
        }
        let mut tokens = Vec::with_capacity(DEFAULT_TREE_SIZE);
        tokens.push(Token {
            kind: SyntaxKind::EndOfFile,
            attached_trivia: AttachedTrivia::new(false, false, 0),
            end: TextSize::new(0),
            parent: NodeId(0),
        });
        tokens.reserve(token_cap.saturating_sub(tokens.len()));
        Self {
            nodes,
            node_children: Vec::with_capacity(DEFAULT_TREE_SIZE),
            tokens,
            text,

            node_children_pool,
            opened,
            text_cursor: TextSize::new(0),

            trivia: TriviaState::new(),
        }
    }

    pub(crate) fn abandon(&mut self) {
        self.trivia.abandon();
        self.opened.clear();
    }

    fn with_trivia<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TriviaState, &mut TreeBuilder),
    {
        let mut trivia = std::mem::take(&mut self.trivia);
        f(&mut trivia, self);
        self.trivia = trivia;
    }

    fn flush_pending(&mut self) {
        self.with_trivia(|trivia, builder| trivia.flush_into(builder));
    }

    fn last_opened(&self) -> &Frame {
        self.opened.last().expect("no opened nodes?")
    }

    fn last_opened_mut(&mut self) -> &mut Frame {
        self.opened.last_mut().expect("no opened nodes?")
    }

    #[inline]
    fn bump_range(dst: &mut Option<(TokenId, TokenId)>, new: (TokenId, TokenId)) {
        match dst {
            None => *dst = Some(new),
            Some((_first, last)) => *last = new.1,
        }
    }

    #[inline]
    fn last_opened_id(&self) -> NodeId {
        self.last_opened().id
    }

    fn flush_children<T>(
        arena: &mut Vec<T>,
        pool: &mut VecPool<T>,
        mut children: Vec<T>,
    ) -> std::ops::Range<usize> {
        let start = arena.len();
        arena.append(&mut children);
        let end = arena.len();
        pool.give(children);
        start..end
    }

    fn close_node_frame(&mut self, id: NodeId, children: Vec<NodeOrTokenRef>) {
        let range =
            Self::flush_children(&mut self.node_children, &mut self.node_children_pool, children);
        self.nodes[id.0].children = range;
    }

    fn advance_text(&mut self, len: TextSize) -> TextSize {
        self.text_cursor += len;
        debug_assert!(self.text.is_char_boundary(usize::from(self.text_cursor)));
        self.text_cursor
    }

    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.start_node_reserve(kind, 0);
    }

    pub fn start_node_reserve(&mut self, kind: SyntaxKind, estimated_children: usize) {
        let parent = self.opened.last().map(|frame| frame.id);
        let new_node = NodeId(self.nodes.len());
        self.nodes.push(Node {
            parent,
            kind,
            children: 0..0,
            first_token: TokenId(0),
            last_token: TokenId(0),
        });
        if parent.is_some() {
            self.push_child_node(new_node);
        }
        let mut children = self.node_children_pool.take();
        children.reserve(estimated_children);
        self.opened.push(Frame { id: new_node, children, token_range: None });
    }

    fn close_top_frame(&mut self) {
        let Frame { id, children, token_range } = self.opened.pop().expect("no opened nodes?");
        let (first, last) = token_range.expect("node without tokens");
        let node = &mut self.nodes[id.0];
        node.first_token = first;
        node.last_token = last;
        self.close_node_frame(id, children);
        if let Some(parent) = self.opened.last_mut() {
            Self::bump_range(&mut parent.token_range, (first, last));
        }
    }

    pub fn finish_node(&mut self) {
        self.close_top_frame();
    }

    pub fn emit_token_with_trivia(
        &mut self,
        leading_trivia: impl ExactSizeIterator<Item = (SyntaxKind, TextSize)>,
        kind: SyntaxKind,
        token_len: TextSize,
        trailing_trivia: impl ExactSizeIterator<Item = (SyntaxKind, TextSize)>,
    ) {
        let parent = self.last_opened_id();
        let leading_trivia_len = leading_trivia.len();
        let trailing_trivia_len = trailing_trivia.len();
        self.tokens.reserve(leading_trivia_len + 1 + trailing_trivia_len);
        let first_token_index = self.tokens.len();
        for (kind, text_len) in leading_trivia {
            let end = self.advance_text(text_len);
            self.tokens.push(Token {
                kind,
                attached_trivia: AttachedTrivia::new(false, false, 0),
                end,
                parent,
            });
        }
        let token = TokenId(self.tokens.len());
        let token_end = self.advance_text(token_len);
        self.tokens.push(Token {
            kind,
            attached_trivia: AttachedTrivia::new(
                leading_trivia_len != 0,
                trailing_trivia_len != 0,
                leading_trivia_len,
            ),
            end: token_end,
            parent,
        });
        self.push_child_token(token);
        for (kind, text_len) in trailing_trivia {
            let end = self.advance_text(text_len);
            self.tokens.push(Token {
                kind,
                attached_trivia: AttachedTrivia::new(false, false, trailing_trivia_len),
                end,
                parent,
            });
        }
        let last_token_index = first_token_index + leading_trivia_len + trailing_trivia_len;
        let first_token = TokenId(first_token_index);
        let last_token = TokenId(last_token_index);

        let emitted = (first_token, last_token);
        Self::bump_range(&mut self.last_opened_mut().token_range, emitted);
    }

    fn push_child_node(&mut self, node: NodeId) {
        self.last_opened_mut().children.push(NodeOrTokenRef::Node(node));
    }

    fn push_child_token(&mut self, token: TokenId) {
        self.last_opened_mut().children.push(NodeOrTokenRef::Token(token));
    }

    pub fn finish(self) -> SyntaxTree {
        let mut builder = self;
        builder.flush_pending();
        SyntaxTree { tree: builder.finish_impl() }
    }

    fn finish_impl(mut self) -> Tree {
        match self.opened.len() {
            0 => {
                assert!(!self.nodes.is_empty(), "no root node");
            }
            1 => self.close_top_frame(),
            _ => panic!("unbalanced nodes in TreeBuilder::finish()"),
        }

        let tree = TreeInner {
            text: std::mem::take(&mut self.text),
            tokens: std::mem::take(&mut self.tokens),
            nodes: Nodes {
                nodes: std::mem::take(&mut self.nodes),
                node_children: std::mem::take(&mut self.node_children),
            },
        };
        self.opened.clear();
        Tree(Rc::new(tree))
    }
}

impl EventSink for TreeBuilder {
    fn enter_node(&mut self, kind: SyntaxKind, estimated_children: usize) {
        self.flush_pending();
        self.start_node_reserve(kind, estimated_children);
    }

    fn exit_node(&mut self, _kind: SyntaxKind) {
        self.flush_pending();
        self.finish_node();
    }

    fn token(&mut self, token: &ParserToken) {
        self.with_trivia(|trivia, builder| trivia.on_token(builder, token));
    }
}

#[derive(Debug)]
pub enum ParseError {
    UnknownDialect(DialectKind),
    Lex(Vec<SQLLexError>),
    Parse(SQLParseError),
    Unparsable(Vec<TextRange>),
    Panic(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownDialect(kind) => {
                write!(f, "dialect not available in sqruff-lib-dialects: {kind:?}")
            }
            ParseError::Lex(errors) => {
                write!(f, "lex error: ")?;
                for (idx, err) in errors.iter().enumerate() {
                    if idx > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", err.message)?;
                }
                Ok(())
            }
            ParseError::Parse(err) => write!(f, "parse error: {}", err.description),
            ParseError::Unparsable(ranges) => {
                if ranges.len() == 1 {
                    write!(f, "unparsable section")
                } else {
                    write!(f, "unparsable sections ({})", ranges.len())
                }
            }
            ParseError::Panic(message) => write!(f, "parser panicked: {message}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(sql: &str, dialect_kind: DialectKind) -> Result<SyntaxTree, ParseError> {
    let dialect = kind_to_dialect(&dialect_kind).ok_or(ParseError::UnknownDialect(dialect_kind))?;
    let lexer = Lexer::from(&dialect);
    let (tokens, lex_errors) = lexer.lex_str(sql);
    if !lex_errors.is_empty() {
        return Err(ParseError::Lex(lex_errors));
    }

    let parser = Parser::from(&dialect);
    let mut sink = TreeBuilder::new_rootless_with_caps(sql, tokens.len().saturating_add(1));
    let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse_with_sink(&tokens, &mut sink)
    }));
    match parse_result {
        Ok(Ok(())) => {
            let tree = sink.finish();
            let ranges = collect_unparsable_ranges(&tree);
            if ranges.is_empty() { Ok(tree) } else { Err(ParseError::Unparsable(ranges)) }
        }
        Ok(Err(err)) => {
            sink.abandon();
            Err(ParseError::Parse(err))
        }
        Err(panic) => {
            sink.abandon();
            Err(ParseError::Panic(panic_message(panic)))
        }
    }
}

fn collect_unparsable_ranges(tree: &SyntaxTree) -> Vec<TextRange> {
    tree.root()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::Unparsable)
        .map(|node| node.text_range())
        .collect()
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "parser panicked".to_string()
    }
}

// Text utilities for edits and offsets.

/// A single textual edit represented as a byte range replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: TextRange,
    pub replacement: String,
}

impl TextEdit {
    /// Replace the given range with `replacement`.
    pub fn replace(range: TextRange, replacement: impl Into<String>) -> Self {
        Self { range, replacement: replacement.into() }
    }

    /// Insert `text` at the given offset.
    pub fn insert(offset: TextSize, text: impl Into<String>) -> Self {
        Self::replace(TextRange::new(offset, offset), text)
    }

    /// Delete the given range.
    pub fn delete(range: TextRange) -> Self {
        Self::replace(range, String::new())
    }
}

/// A labeled set of edits, suitable for code actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub edits: Vec<TextEdit>,
}

impl Fix {
    /// Create a new fix with the given title and edits.
    pub fn new(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self { title: title.into(), edits }
    }

    /// Create a fix with a single edit.
    pub fn single(title: impl Into<String>, edit: TextEdit) -> Self {
        Self::new(title, vec![edit])
    }
}

/// Errors returned by `apply_edits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    /// Edits overlap or are not strictly ordered.
    Overlap,
    /// An edit refers to offsets outside the input text.
    OutOfBounds,
    /// An edit splits a UTF-8 code point.
    InvalidBoundary,
}

/// Apply a set of non-overlapping edits to the input text.
///
/// Edits are applied in order by range start, and must not overlap.
pub fn apply_edits(text: &str, mut edits: Vec<TextEdit>) -> Result<String, EditError> {
    if edits.is_empty() {
        return Ok(text.to_string());
    }

    edits.sort_by_key(|edit| edit.range.start());

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;

    for edit in edits {
        let start = usize::from(edit.range.start());
        let end = usize::from(edit.range.end());

        if start < cursor {
            return Err(EditError::Overlap);
        }
        if end > text.len() {
            return Err(EditError::OutOfBounds);
        }
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(EditError::InvalidBoundary);
        }

        out.push_str(&text[cursor..start]);
        out.push_str(&edit.replacement);
        cursor = end;
    }

    out.push_str(&text[cursor..]);
    Ok(out)
}
