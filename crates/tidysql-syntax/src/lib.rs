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

pub(crate) type ElementId = NodeOrToken<NodeId, TokenId>;

#[derive(Clone, Copy, GetSize)]
pub(crate) struct TokenData {
    #[get_size(ignore)]
    pub(crate) kind: SyntaxKind,
    pub(crate) trivia: TriviaAttachment,
    #[get_size(ignore)]
    pub(crate) end: TextSize,
    pub(crate) parent_id: NodeId,
}

#[derive(Clone, Copy, GetSize)]
pub(crate) struct TriviaAttachment {
    has_leading_trivia: bool,
    has_trailing_trivia: bool,
    trivia_count: u16,
}

impl TriviaAttachment {
    #[inline]
    pub(crate) fn new(
        has_leading_trivia: bool,
        has_trailing_trivia: bool,
        trivia_count: usize,
    ) -> TriviaAttachment {
        TriviaAttachment {
            has_leading_trivia,
            has_trailing_trivia,
            trivia_count: u16::try_from(trivia_count).expect("trivia_count must fit into u16"),
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
    pub(crate) fn trivia_count(self) -> usize {
        self.trivia_count as usize
    }
}

impl TokenId {
    #[inline]
    pub(crate) fn get(self, tree: &TreeData) -> &TokenData {
        &tree.tokens[self.0]
    }

    #[inline]
    fn prev_or_sentinel(self, tree: &TreeData) -> &TokenData {
        &tree.tokens[self.0 - 1]
    }

    #[inline]
    pub(crate) fn start(self, tree: &TreeData) -> TextSize {
        self.prev_or_sentinel(tree).end
    }

    #[inline]
    pub(crate) fn end(self, tree: &TreeData) -> TextSize {
        self.get(tree).end
    }

    #[inline]
    pub(crate) fn text_range(self, tree: &TreeData) -> TextRange {
        TextRange::new(self.start(tree), self.get(tree).end)
    }

    #[inline]
    pub(crate) fn text(self, tree: &TreeData) -> &str {
        &tree.text[self.text_range(tree)]
    }

    #[inline]
    pub(crate) fn prev_token(self) -> Option<Self> {
        if self.0 <= 1 { None } else { Some(TokenId(self.0 - 1)) }
    }

    #[inline]
    pub(crate) fn next_token(self, tree: &TreeData) -> Option<Self> {
        let next = self.0 + 1;
        if next >= tree.tokens.len() { None } else { Some(TokenId(next)) }
    }

    #[inline]
    pub(crate) fn leading_trivia(self, tree: &TreeData) -> TokenIdIter {
        if !self.get(tree).trivia.has_leading_trivia() {
            return TokenIdIter::empty();
        }

        let trivia_count = self.get(tree).trivia.trivia_count();
        let trivia_start = self.0 - trivia_count;
        TokenIdIter::new(trivia_start, trivia_count)
    }

    #[inline]
    pub(crate) fn trailing_trivia(self, tree: &TreeData) -> TokenIdIter {
        if !self.get(tree).trivia.has_trailing_trivia() {
            return TokenIdIter::empty();
        }

        let trivia_start = self.0 + 1;
        let trivia_count = tree.tokens[trivia_start].trivia.trivia_count();
        TokenIdIter::new(trivia_start, trivia_count)
    }

    #[inline]
    pub(crate) fn parent(self, tree: &TreeData) -> NodeId {
        self.get(tree).parent_id
    }
}

#[derive(Clone)]
pub(crate) struct TokenIdIter {
    current: usize,
    end: usize,
}

impl TokenIdIter {
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

impl Iterator for TokenIdIter {
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

impl DoubleEndedIterator for TokenIdIter {
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
pub(crate) struct NodeStore {
    pub(crate) nodes: Vec<NodeData>,
    pub(crate) node_children: Vec<ElementId>,
}

impl NodeStore {}

#[derive(GetSize)]
pub(crate) struct NodeData {
    pub(crate) parent_id: Option<NodeId>,
    pub(crate) child_range: Range<usize>,
    #[get_size(ignore)]
    pub(crate) kind: SyntaxKind,
    pub(crate) first_token_id: TokenId,
    pub(crate) last_token_id: TokenId,
}

impl NodeData {
    #[inline]
    pub(crate) fn text_range(&self, tree: &TreeData) -> TextRange {
        TextRange::new(self.first_token(tree).start(tree), self.last_token(tree).end(tree))
    }

    #[inline]
    pub(crate) fn text<'a>(&self, tree: &'a TreeData) -> &'a str {
        let range = self.text_range(tree);
        &tree.text[range]
    }

    #[inline]
    pub(crate) fn first_token(&self, _tree: &TreeData) -> TokenId {
        self.first_token_id
    }

    #[inline]
    pub(crate) fn last_token(&self, _tree: &TreeData) -> TokenId {
        self.last_token_id
    }

    #[inline]
    pub(crate) fn parent(&self) -> Option<NodeId> {
        self.parent_id
    }

    #[inline]
    pub(crate) fn children<'a>(&self, tree: &'a TreeData) -> &'a [ElementId] {
        &tree.node_store.node_children[self.child_range.clone()]
    }

    #[inline]
    fn token_slice<'a>(&self, tree: &'a TreeData) -> &'a [TokenData] {
        let start = self.first_token_id.0;
        let end = self.last_token_id.0.saturating_add(1);
        &tree.tokens[start..end]
    }

    #[inline]
    pub(crate) fn token_at_offset(
        &self,
        tree: &TreeData,
        offset: TextSize,
    ) -> TokenAtOffset<TokenId> {
        let range = self.text_range(tree);
        if offset < range.start() || offset >= range.end() {
            return TokenAtOffset::None;
        }

        let token_slice = self.token_slice(tree);
        let index = token_slice.partition_point(|token_data| token_data.end <= offset);
        let token_index = self.first_token_id.0 + index;
        if token_index >= tree.tokens.len() {
            return TokenAtOffset::None;
        }
        let right_token = TokenId(token_index);
        if right_token.end(tree) <= offset {
            return TokenAtOffset::None;
        }
        if let Some(left_token) = right_token.prev_token()
            && left_token.end(tree) == offset
        {
            TokenAtOffset::Between(left_token, right_token)
        } else {
            TokenAtOffset::Single(right_token)
        }
    }

    #[inline]
    pub(crate) fn covering_element(&self, tree: &TreeData, range: TextRange) -> ElementId {
        let token_id = self
            .token_at_offset(tree, range.start())
            .right_biased()
            .expect("range is not inside the node");
        if token_id.text_range(tree).contains_range(range) {
            return ElementId::Token(token_id);
        }
        let mut current_node_id = token_id.parent(tree);
        loop {
            let node_data = &tree.node_store.nodes[current_node_id.0];
            if node_data.text_range(tree).contains_range(range) {
                return ElementId::Node(current_node_id);
            }
            current_node_id = node_data.parent_id.expect("range is not inside the node");
        }
    }
}

#[derive(Clone, GetSize)]
pub(crate) struct SharedTree(pub(crate) Rc<TreeData>);

#[derive(GetSize)]
pub(crate) struct TreeData {
    pub(crate) text: String,
    pub(crate) tokens: Vec<TokenData>,
    pub(crate) node_store: NodeStore,
}

#[derive(GetSize)]
pub struct SyntaxTree {
    pub(crate) tree: SharedTree,
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
        SyntaxNode { tree: self.tree.clone(), node_id: NodeId(0) }
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
    tree: SharedTree,
    token_id: TokenId,
}

impl SyntaxToken {
    #[inline]
    pub fn id(&self) -> TokenId {
        self.token_id
    }

    #[inline]
    pub fn kind(&self) -> SyntaxKind {
        self.token_id.get(&self.tree.0).kind
    }

    #[inline]
    pub fn text_range(&self) -> TextRange {
        self.token_id.text_range(&self.tree.0)
    }

    #[inline]
    pub fn prev_token(&self) -> Option<Self> {
        Some(Self { tree: self.tree.clone(), token_id: self.token_id.prev_token()? })
    }

    #[inline]
    pub fn next_token(&self) -> Option<Self> {
        Some(Self { tree: self.tree.clone(), token_id: self.token_id.next_token(&self.tree.0)? })
    }

    #[inline]
    pub fn leading_trivia(&self) -> TriviaIter {
        TriviaIter { tree: self.tree.clone(), tokens: self.token_id.leading_trivia(&self.tree.0) }
    }

    #[inline]
    pub fn trailing_trivia(&self) -> TriviaIter {
        TriviaIter { tree: self.tree.clone(), tokens: self.token_id.trailing_trivia(&self.tree.0) }
    }

    #[inline]
    pub fn text(&self) -> &str {
        self.token_id.text(&self.tree.0)
    }

    #[inline]
    pub fn text_range_including_trivia(&self) -> TextRange {
        let first_token = self.leading_trivia().next().unwrap_or_else(|| self.clone());
        let last_token = self.trailing_trivia().next_back().unwrap_or_else(|| self.clone());
        let tree = &self.tree.0;
        TextRange::new(first_token.token_id.start(tree), last_token.token_id.end(tree))
    }

    #[inline]
    pub fn text_including_trivia(&self) -> &str {
        &self.tree.0.text[self.text_range_including_trivia()]
    }

    #[inline]
    pub fn parent(&self) -> SyntaxNode {
        SyntaxNode { tree: self.tree.clone(), node_id: self.token_id.parent(&self.tree.0) }
    }

    #[inline]
    pub fn ancestors(&self) -> impl Iterator<Item = SyntaxNode> + Clone {
        std::iter::successors(Some(self.parent()), |node: &SyntaxNode| node.parent())
    }
}

#[derive(Clone)]
pub struct TriviaIter {
    tree: SharedTree,
    tokens: TokenIdIter,
}

impl Iterator for TriviaIter {
    type Item = SyntaxToken;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(SyntaxToken { tree: self.tree.clone(), token_id: self.tokens.next()? })
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
        Some(SyntaxToken { tree: self.tree.clone(), token_id: self.tokens.next_back()? })
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
    tree: SharedTree,
    node_id: NodeId,
}

impl SyntaxNode {
    #[inline]
    fn node_data(&self) -> &NodeData {
        &self.tree.0.node_store.nodes[self.node_id.0]
    }

    #[inline]
    pub(crate) fn shares_tree_with(&self, other: &SyntaxNode) -> bool {
        std::rc::Rc::ptr_eq(&self.tree.0, &other.tree.0)
    }

    #[inline]
    pub fn kind(&self) -> SyntaxKind {
        self.node_data().kind
    }

    #[inline]
    pub fn first_token(&self) -> SyntaxToken {
        SyntaxToken {
            tree: self.tree.clone(),
            token_id: self.node_data().first_token(&self.tree.0),
        }
    }

    #[inline]
    pub fn last_token(&self) -> SyntaxToken {
        SyntaxToken { tree: self.tree.clone(), token_id: self.node_data().last_token(&self.tree.0) }
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
        Some(Self { tree: self.tree.clone(), node_id: self.node_data().parent()? })
    }

    #[inline]
    pub fn try_token_child(&self, index: usize) -> Option<SyntaxToken> {
        match *self.node_data().children(&self.tree.0).get(index)? {
            ElementId::Token(token_id) => Some(SyntaxToken { tree: self.tree.clone(), token_id }),
            ElementId::Node(_) => None,
        }
    }

    #[inline]
    #[track_caller]
    pub fn token_child(&self, index: usize) -> SyntaxToken {
        match self.try_token_child(index) {
            Some(token) => token,
            None => expected_token(index),
        }
    }

    #[inline]
    pub fn try_node_child(&self, index: usize) -> Option<SyntaxNode> {
        match *self.node_data().children(&self.tree.0).get(index)? {
            ElementId::Node(node_id) => Some(SyntaxNode { tree: self.tree.clone(), node_id }),
            ElementId::Token(_) => None,
        }
    }

    #[inline]
    #[track_caller]
    pub fn node_child(&self, index: usize) -> SyntaxNode {
        match self.try_node_child(index) {
            Some(node) => node,
            None => expected_node(index),
        }
    }

    #[inline]
    pub fn try_child(&self, index: usize) -> Option<SyntaxElement> {
        self.node_data()
            .children(&self.tree.0)
            .get(index)
            .copied()
            .map(|element_id| resolve_element_id(&self.tree, element_id))
    }

    #[inline]
    #[track_caller]
    pub fn child(&self, index: usize) -> SyntaxElement {
        resolve_element_id(&self.tree, self.node_data().children(&self.tree.0)[index])
    }

    #[inline]
    pub fn ancestors(&self) -> impl Iterator<Item = Self> + Clone {
        std::iter::successors(Some(self.clone()), |node| node.parent())
    }

    #[inline]
    pub fn children_with_tokens(&self) -> ChildrenWithTokens {
        ChildrenWithTokens { tree: self.tree.clone(), range: self.node_data().child_range.clone() }
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
            TokenAtOffset::Single(token_id) => {
                TokenAtOffset::Single(SyntaxToken { tree: self.tree.clone(), token_id })
            }
            TokenAtOffset::Between(left, right) => TokenAtOffset::Between(
                SyntaxToken { tree: self.tree.clone(), token_id: left },
                SyntaxToken { tree: self.tree.clone(), token_id: right },
            ),
        }
    }

    #[inline]
    pub fn covering_element(&self, range: TextRange) -> SyntaxElement {
        resolve_element_id(&self.tree, self.node_data().covering_element(&self.tree.0, range))
    }
}

#[inline]
fn resolve_element_id(tree: &SharedTree, child: ElementId) -> SyntaxElement {
    match child {
        ElementId::Token(token_id) => {
            SyntaxElement::Token(SyntaxToken { tree: tree.clone(), token_id })
        }
        ElementId::Node(node_id) => SyntaxElement::Node(SyntaxNode { tree: tree.clone(), node_id }),
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
    tree: SharedTree,
    range: std::ops::Range<usize>,
}

impl ChildrenWithTokens {
    #[inline]
    fn element_at(&self, index: usize) -> SyntaxElement {
        resolve_element_id(&self.tree, self.tree.0.node_store.node_children[index])
    }
}

impl Iterator for ChildrenWithTokens {
    type Item = SyntaxElement;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().map(|index| self.element_at(index))
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
        self.range.next_back().map(|index| self.element_at(index))
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
            SyntaxElement::Node(node) => Some(node),
            SyntaxElement::Token(_) => None,
        }
    }

    #[inline]
    fn into_nodes(self) -> impl Iterator<Item = SyntaxNode> {
        self.inner.filter_map(|child| match child {
            SyntaxElement::Node(node) => Some(node),
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
        self.into_nodes().fold(init, f)
    }

    #[inline]
    fn for_each<F>(self, f: F)
    where
        Self: Sized,
        F: FnMut(Self::Item),
    {
        self.into_nodes().for_each(f);
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
        self.inner.find_map(|event| match event {
            WalkEventWithTokens::EnterNode(node) => Some(WalkEvent::Enter(node)),
            WalkEventWithTokens::LeaveNode(node) => Some(WalkEvent::Leave(node)),
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
        let (_, child_iter) = self.stack.last_mut()?;
        match child_iter.next() {
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
        self.node_id == other.node_id && self.shares_tree_with(other)
    }
}

impl Eq for SyntaxNode {}

impl std::hash::Hash for SyntaxNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tree_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        tree_ptr.hash(state);
        self.node_id.hash(state);
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
        self_ptr.cmp(&other_ptr).then_with(|| self.node_id.cmp(&other.node_id))
    }
}

impl PartialEq for SyntaxToken {
    fn eq(&self, other: &Self) -> bool {
        self.token_id == other.token_id && std::rc::Rc::ptr_eq(&self.tree.0, &other.tree.0)
    }
}

impl Eq for SyntaxToken {}

impl std::hash::Hash for SyntaxToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tree_ptr = std::rc::Rc::as_ptr(&self.tree.0) as usize;
        tree_ptr.hash(state);
        self.token_id.hash(state);
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
        self_ptr.cmp(&other_ptr).then_with(|| self.token_id.cmp(&other.token_id))
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

struct OpenNodeFrame {
    node_id: NodeId,
    children: Vec<ElementId>,
    token_bounds: Option<(TokenId, TokenId)>,
}

struct VecPool<T> {
    pool: Vec<Vec<T>>,
    default_capacity: usize,
}

struct PendingToken {
    kind: SyntaxKind,
    text_len: TextSize,
}

#[derive(Default)]
struct TriviaState {
    pending: Option<PendingToken>,
    leading_trivia: Vec<(SyntaxKind, TextSize)>,
    trailing_trivia: Vec<(SyntaxKind, TextSize)>,
}

impl TriviaState {
    fn new() -> Self {
        Self {
            pending: None,
            leading_trivia: Vec::with_capacity(8),
            trailing_trivia: Vec::with_capacity(8),
        }
    }

    #[allow(dead_code)]
    fn abandon(&mut self) {
        self.pending = None;
        self.leading_trivia.clear();
        self.trailing_trivia.clear();
    }

    fn flush_pending_into(&mut self, builder: &mut TreeBuilder) {
        let Some(pending) = self.pending.take() else {
            return;
        };

        let mut leading_trivia = std::mem::take(&mut self.leading_trivia);
        let mut trailing_trivia = std::mem::take(&mut self.trailing_trivia);

        builder.emit_token_with_trivia(
            leading_trivia.drain(..),
            pending.kind,
            pending.text_len,
            trailing_trivia.drain(..),
        );

        self.leading_trivia = leading_trivia;
        self.trailing_trivia = trailing_trivia;
    }

    fn push_trivia(&mut self, token: &ParserToken) {
        let text_len = TextSize::of(token.raw.as_str());
        if self.pending.is_some() {
            self.trailing_trivia.push((token.kind, text_len));
        } else {
            self.leading_trivia.push((token.kind, text_len));
        }
    }

    fn emit_meta_token(&mut self, builder: &mut TreeBuilder, token: &ParserToken) {
        let text_len = TextSize::of(token.raw.as_str());
        let mut leading_trivia = std::mem::take(&mut self.leading_trivia);

        builder.emit_token_with_trivia(
            leading_trivia.drain(..),
            token.kind,
            text_len,
            std::iter::empty(),
        );

        self.leading_trivia = leading_trivia;
    }

    fn buffer_token(&mut self, token: &ParserToken) {
        self.pending =
            Some(PendingToken { kind: token.kind, text_len: TextSize::of(token.raw.as_str()) });
    }

    fn handle_token(&mut self, builder: &mut TreeBuilder, token: &ParserToken) {
        if token.is_whitespace() || token.is_comment() {
            self.push_trivia(token);
            return;
        }

        if token.is_meta() {
            self.flush_pending_into(builder);
            self.emit_meta_token(builder, token);
            return;
        }

        self.flush_pending_into(builder);
        self.buffer_token(token);
    }
}

impl<T> VecPool<T> {
    fn new(pool_cap: usize, default_capacity: usize) -> Self {
        Self { pool: Vec::with_capacity(pool_cap), default_capacity }
    }

    fn acquire(&mut self) -> Vec<T> {
        self.pool.pop().unwrap_or_else(|| Vec::with_capacity(self.default_capacity))
    }

    fn release(&mut self, mut vec: Vec<T>) {
        vec.clear();
        self.pool.push(vec);
    }
}

pub struct TreeBuilder {
    nodes: Vec<NodeData>,
    node_children: Vec<ElementId>,
    tokens: Vec<TokenData>,
    text: String,
    unparsable_ranges: Vec<TextRange>,

    node_children_pool: VecPool<ElementId>,
    open_frames: Vec<OpenNodeFrame>,
    text_cursor: TextSize,

    trivia: TriviaState,
}

impl Drop for TreeBuilder {
    fn drop(&mut self) {
        if !std::thread::panicking() && !self.open_frames.is_empty() {
            panic!("you should call `TreeBuilder::finish()`");
        }
    }
}

const DEFAULT_TREE_DEPTH: usize = 128;
const DEFAULT_CHILDREN_CAPACITY: usize = 10;
const MIN_TREE_CAPACITY: usize = 16;

impl TreeBuilder {
    pub(crate) fn new_rootless_with_token_capacity(
        source: impl Into<String>,
        token_cap: usize,
    ) -> Self {
        Self::new_impl(source.into(), None, token_cap)
    }

    fn new_impl(text: String, root_kind: Option<SyntaxKind>, token_cap: usize) -> Self {
        let tree_cap = token_cap.max(MIN_TREE_CAPACITY);
        let mut nodes = Vec::with_capacity(tree_cap);
        let mut node_children_pool = VecPool::new(DEFAULT_TREE_DEPTH, DEFAULT_CHILDREN_CAPACITY);
        let mut open_frames = Vec::with_capacity(DEFAULT_TREE_DEPTH);
        if let Some(kind) = root_kind {
            nodes.push(NodeData {
                parent_id: None,
                kind,
                child_range: 0..0,
                first_token_id: TokenId(0),
                last_token_id: TokenId(0),
            });
            open_frames.push(OpenNodeFrame {
                node_id: NodeId(0),
                children: node_children_pool.acquire(),
                token_bounds: None,
            });
        }
        let mut tokens = Vec::with_capacity(tree_cap);
        tokens.push(TokenData {
            kind: SyntaxKind::EndOfFile,
            trivia: TriviaAttachment::new(false, false, 0),
            end: TextSize::new(0),
            parent_id: NodeId(0),
        });
        Self {
            nodes,
            node_children: Vec::with_capacity(tree_cap),
            tokens,
            text,
            unparsable_ranges: Vec::new(),

            node_children_pool,
            open_frames,
            text_cursor: TextSize::new(0),

            trivia: TriviaState::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn abandon(&mut self) {
        self.trivia.abandon();
        self.open_frames.clear();
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
        self.with_trivia(|trivia, builder| trivia.flush_pending_into(builder));
    }

    fn current_frame(&self) -> &OpenNodeFrame {
        self.open_frames.last().expect("no opened nodes?")
    }

    fn current_frame_mut(&mut self) -> &mut OpenNodeFrame {
        self.open_frames.last_mut().expect("no opened nodes?")
    }

    #[inline]
    fn extend_token_bounds(dst: &mut Option<(TokenId, TokenId)>, new_bounds: (TokenId, TokenId)) {
        match dst {
            None => *dst = Some(new_bounds),
            Some((_first_token_id, last_token_id)) => *last_token_id = new_bounds.1,
        }
    }

    #[inline]
    fn current_node_id(&self) -> NodeId {
        self.current_frame().node_id
    }

    fn flush_children<T>(
        arena: &mut Vec<T>,
        pool: &mut VecPool<T>,
        mut children: Vec<T>,
    ) -> std::ops::Range<usize> {
        let start = arena.len();
        arena.append(&mut children);
        let end = arena.len();
        pool.release(children);
        start..end
    }

    fn store_node_children(&mut self, node_id: NodeId, children: Vec<ElementId>) {
        let range =
            Self::flush_children(&mut self.node_children, &mut self.node_children_pool, children);
        self.nodes[node_id.0].child_range = range;
    }

    fn advance_text(&mut self, len: TextSize) -> TextSize {
        self.text_cursor += len;
        debug_assert!(self.text.is_char_boundary(usize::from(self.text_cursor)));
        self.text_cursor
    }

    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.start_node_with_capacity(kind, 0);
    }

    pub fn start_node_with_capacity(&mut self, kind: SyntaxKind, estimated_children: usize) {
        let parent_id = self.open_frames.last().map(|frame| frame.node_id);
        let node_id = NodeId(self.nodes.len());
        self.nodes.push(NodeData {
            parent_id,
            kind,
            child_range: 0..0,
            first_token_id: TokenId(0),
            last_token_id: TokenId(0),
        });
        if parent_id.is_some() {
            self.push_child_node(node_id);
        }
        let mut children = self.node_children_pool.acquire();
        children.reserve(estimated_children);
        self.open_frames.push(OpenNodeFrame { node_id, children, token_bounds: None });
    }

    fn finish_current_node(&mut self) {
        let OpenNodeFrame { node_id, children, token_bounds } =
            self.open_frames.pop().expect("no opened nodes?");
        let (first_token_id, last_token_id) = token_bounds.expect("node without tokens");
        let kind = self.nodes[node_id.0].kind;
        let node_data = &mut self.nodes[node_id.0];
        node_data.first_token_id = first_token_id;
        node_data.last_token_id = last_token_id;
        self.store_node_children(node_id, children);
        if kind == SyntaxKind::Unparsable {
            debug_assert!(first_token_id.0 > 0, "real tokens should follow the sentinel EOF token");
            self.unparsable_ranges.push(TextRange::new(
                self.tokens[first_token_id.0 - 1].end,
                self.tokens[last_token_id.0].end,
            ));
        }
        if let Some(parent_frame) = self.open_frames.last_mut() {
            Self::extend_token_bounds(
                &mut parent_frame.token_bounds,
                (first_token_id, last_token_id),
            );
        }
    }

    pub fn finish_node(&mut self) {
        self.finish_current_node();
    }

    pub fn emit_token_with_trivia(
        &mut self,
        leading_trivia: impl ExactSizeIterator<Item = (SyntaxKind, TextSize)>,
        kind: SyntaxKind,
        token_len: TextSize,
        trailing_trivia: impl ExactSizeIterator<Item = (SyntaxKind, TextSize)>,
    ) {
        let parent_id = self.current_node_id();
        let leading_trivia_count = leading_trivia.len();
        let trailing_trivia_count = trailing_trivia.len();
        self.tokens.reserve(leading_trivia_count + 1 + trailing_trivia_count);
        let first_token_index = self.tokens.len();
        for (kind, text_len) in leading_trivia {
            let end = self.advance_text(text_len);
            self.tokens.push(TokenData {
                kind,
                trivia: TriviaAttachment::new(false, false, 0),
                end,
                parent_id,
            });
        }
        let token_id = TokenId(self.tokens.len());
        let token_end = self.advance_text(token_len);
        self.tokens.push(TokenData {
            kind,
            trivia: TriviaAttachment::new(
                leading_trivia_count != 0,
                trailing_trivia_count != 0,
                leading_trivia_count,
            ),
            end: token_end,
            parent_id,
        });
        self.push_child_token(token_id);
        for (kind, text_len) in trailing_trivia {
            let end = self.advance_text(text_len);
            self.tokens.push(TokenData {
                kind,
                trivia: TriviaAttachment::new(false, false, trailing_trivia_count),
                end,
                parent_id,
            });
        }
        let last_token_index = first_token_index + leading_trivia_count + trailing_trivia_count;
        let first_token_id = TokenId(first_token_index);
        let last_token_id = TokenId(last_token_index);

        let emitted_bounds = (first_token_id, last_token_id);
        Self::extend_token_bounds(&mut self.current_frame_mut().token_bounds, emitted_bounds);
    }

    fn push_child_node(&mut self, node_id: NodeId) {
        self.current_frame_mut().children.push(ElementId::Node(node_id));
    }

    fn push_child_token(&mut self, token_id: TokenId) {
        self.current_frame_mut().children.push(ElementId::Token(token_id));
    }

    pub fn finish(self) -> SyntaxTree {
        let mut builder = self;
        builder.flush_pending();
        let (tree, _) = builder.finish_impl();
        SyntaxTree { tree }
    }

    fn finish_with_unparsable_ranges(self) -> (SyntaxTree, Vec<TextRange>) {
        let mut builder = self;
        builder.flush_pending();
        let (tree, unparsable_ranges) = builder.finish_impl();
        (SyntaxTree { tree }, unparsable_ranges)
    }

    fn finish_impl(mut self) -> (SharedTree, Vec<TextRange>) {
        match self.open_frames.len() {
            0 => {
                assert!(!self.nodes.is_empty(), "no root node");
            }
            1 => self.finish_current_node(),
            _ => panic!("unbalanced nodes in TreeBuilder::finish()"),
        }

        let tree = TreeData {
            text: std::mem::take(&mut self.text),
            tokens: std::mem::take(&mut self.tokens),
            node_store: NodeStore {
                nodes: std::mem::take(&mut self.nodes),
                node_children: std::mem::take(&mut self.node_children),
            },
        };
        let unparsable_ranges = std::mem::take(&mut self.unparsable_ranges);
        self.open_frames.clear();
        (SharedTree(Rc::new(tree)), unparsable_ranges)
    }
}

impl EventSink for TreeBuilder {
    fn enter_node(&mut self, kind: SyntaxKind, estimated_children: usize) {
        self.flush_pending();
        self.start_node_with_capacity(kind, estimated_children);
    }

    fn exit_node(&mut self, _kind: SyntaxKind) {
        self.flush_pending();
        self.finish_node();
    }

    fn token(&mut self, token: &ParserToken) {
        self.with_trivia(|trivia, builder| trivia.handle_token(builder, token));
    }
}

#[derive(Debug)]
pub enum ParseError {
    UnavailableDialect(DialectKind),
    Lex(Vec<SQLLexError>),
    Parse(SQLParseError),
    UnparsableRanges(Vec<TextRange>),
    ParserPanic(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnavailableDialect(kind) => {
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
            ParseError::UnparsableRanges(ranges) => {
                if ranges.len() == 1 {
                    write!(f, "unparsable section")
                } else {
                    write!(f, "unparsable sections ({})", ranges.len())
                }
            }
            ParseError::ParserPanic(message) => write!(f, "parser panicked: {message}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(sql: impl Into<String>, dialect_kind: DialectKind) -> Result<SyntaxTree, ParseError> {
    let sql = sql.into();
    let dialect =
        kind_to_dialect(&dialect_kind).ok_or(ParseError::UnavailableDialect(dialect_kind))?;
    let lexer = Lexer::from(&dialect);
    let (tokens, lex_errors) = lexer.lex_str(&sql);
    if !lex_errors.is_empty() {
        return Err(ParseError::Lex(lex_errors));
    }

    let parser = Parser::from(&dialect);
    let tree_builder =
        TreeBuilder::new_rootless_with_token_capacity(sql, tokens.len().saturating_add(1));
    let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut tree_builder = tree_builder;
        match parser.parse_with_sink(&tokens, &mut tree_builder) {
            Ok(()) => Ok(tree_builder.finish_with_unparsable_ranges()),
            Err(err) => {
                tree_builder.abandon();
                Err(err)
            }
        }
    }));
    match parse_result {
        Ok(Ok((tree, ranges))) => {
            if ranges.is_empty() {
                Ok(tree)
            } else {
                Err(ParseError::UnparsableRanges(ranges))
            }
        }
        Ok(Err(err)) => Err(ParseError::Parse(err)),
        Err(panic) => Err(ParseError::ParserPanic(panic_message(panic))),
    }
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

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::Overlap => write!(f, "edits overlap or are not strictly ordered"),
            EditError::OutOfBounds => write!(f, "edit refers to offsets outside the input text"),
            EditError::InvalidBoundary => write!(f, "edit splits a UTF-8 code point"),
        }
    }
}

impl std::error::Error for EditError {}

/// Apply a set of non-overlapping edits to the input text.
///
/// Edits are applied in order by range start, and must not overlap.
pub fn apply_edits(text: &str, mut edits: Vec<TextEdit>) -> Result<String, EditError> {
    if edits.is_empty() {
        return Ok(text.to_string());
    }

    edits.sort_by_key(|edit| edit.range.start());

    let mut cursor = 0usize;
    let mut removed_bytes = 0usize;
    let mut replacement_bytes = 0usize;

    for edit in &edits {
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

        removed_bytes += end - start;
        replacement_bytes += edit.replacement.len();
        cursor = end;
    }

    let final_len = text.len() - removed_bytes + replacement_bytes;
    let mut out = String::with_capacity(final_len);
    let mut cursor = 0usize;

    for edit in edits {
        let start = usize::from(edit.range.start());
        let end = usize::from(edit.range.end());

        out.push_str(&text[cursor..start]);
        out.push_str(&edit.replacement);
        cursor = end;
    }

    out.push_str(&text[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(sql: impl Into<String>) -> SyntaxTree {
        parse(sql, DialectKind::Ansi).unwrap()
    }

    fn unsupported_dialect() -> Option<DialectKind> {
        [
            DialectKind::Ansi,
            DialectKind::Athena,
            DialectKind::Bigquery,
            DialectKind::Clickhouse,
            DialectKind::Databricks,
            DialectKind::Duckdb,
            DialectKind::Mysql,
            DialectKind::Postgres,
            DialectKind::Redshift,
            DialectKind::Snowflake,
            DialectKind::Sparksql,
            DialectKind::Sqlite,
            DialectKind::Trino,
            DialectKind::Tsql,
        ]
        .into_iter()
        .find(|kind| kind_to_dialect(kind).is_none())
    }

    fn nested_function_node(tree: &SyntaxTree) -> SyntaxNode {
        tree.root()
            .descendants()
            .find(|node| node.kind() == SyntaxKind::Function)
            .expect("expected a nested function node")
    }

    fn assert_token_texts(tokens: TokenAtOffset, expected: &[&str]) {
        let actual: Vec<_> = tokens.map(|token| token.text().to_string()).collect();
        let expected: Vec<_> = expected.iter().map(|text| (*text).to_string()).collect();
        assert_eq!(actual, expected);
    }

    fn assert_token_kinds(tokens: TokenAtOffset, expected: &[SyntaxKind]) {
        let actual: Vec<_> = tokens.map(|token| token.kind()).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_accepts_borrowed_sql() {
        let result = parse("SELECT 1", DialectKind::Ansi);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_accepts_owned_sql() {
        let result = parse("SELECT 1".to_string(), DialectKind::Ansi);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_returns_error_for_unavailable_dialect_when_a_dialect_is_unavailable() {
        let Some(dialect) = unsupported_dialect() else {
            return;
        };

        let result = parse("SELECT 1", dialect);
        let Err(ParseError::UnavailableDialect(actual)) = result else {
            panic!("expected unavailable dialect parse error");
        };

        assert_eq!(actual, dialect);
    }

    #[test]
    fn parse_whitespace_only_input_succeeds() {
        let tree = parse_ok("   \n\t");
        assert_eq!(tree.text(), "   \n\t");
        assert_eq!(tree.root().text(), "   \n\t");
    }

    #[test]
    fn parse_comment_only_input_succeeds() {
        let tree = parse_ok("-- leading comment");
        assert_eq!(tree.text(), "-- leading comment");
        assert_eq!(tree.root().text(), "-- leading comment");
    }

    #[test]
    fn parse_returns_unparsable_ranges_without_post_walk() {
        let result = parse("SELECT FROM foo", DialectKind::Ansi);
        let Err(ParseError::UnparsableRanges(ranges)) = result else {
            panic!("expected unparsable parse error");
        };

        assert_eq!(ranges.len(), 1);
        assert_eq!(&"SELECT FROM foo"[ranges[0]], "SELECT");
    }

    #[test]
    fn parse_errors_do_not_turn_into_parse_error_panic() {
        let result = parse("SELECT (1", DialectKind::Ansi);
        let Err(ParseError::Parse(error)) = result else {
            panic!("expected parser error, not panic");
        };

        assert!(error.description.contains("closing bracket"));
    }

    #[test]
    fn large_trivia_groups_are_reported_as_parse_panics() {
        let sql = format!("{}SELECT 1", "/*x*/".repeat(usize::from(u16::MAX) + 1));
        let result = parse(sql, DialectKind::Ansi);
        let Err(ParseError::ParserPanic(message)) = result else {
            panic!("expected panic parse error for trivia overflow");
        };

        assert!(message.contains("trivia_count must fit into u16"));
    }

    #[test]
    fn token_at_offset_handles_root_boundaries() {
        let tree = parse_ok("SELECT 1");
        let root = tree.root();

        assert_token_kinds(root.token_at_offset(0.into()), &[SyntaxKind::Keyword]);
        assert_token_kinds(
            root.token_at_offset(6.into()),
            &[SyntaxKind::Indent, SyntaxKind::Whitespace],
        );
        assert!(matches!(root.token_at_offset(8.into()), TokenAtOffset::None));
        assert!(matches!(root.token_at_offset(9.into()), TokenAtOffset::None));
    }

    #[test]
    fn token_at_offset_handles_nested_node_boundaries() {
        let tree = parse_ok("SELECT sum(foo) FROM bar");
        let function = nested_function_node(&tree);
        let range = function.text_range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());

        assert_token_texts(function.token_at_offset((start - 1).try_into().unwrap()), &[]);
        assert_token_kinds(
            function.token_at_offset(range.start()),
            &[SyntaxKind::Indent, SyntaxKind::Whitespace],
        );
        assert_token_texts(function.token_at_offset(10.into()), &["sum", "("]);
        assert!(matches!(function.token_at_offset(range.end()), TokenAtOffset::None));
        assert_token_texts(function.token_at_offset((end + 1).try_into().unwrap()), &[]);
    }

    #[test]
    fn apply_edits_handles_equal_length_replacements() {
        let result = apply_edits(
            "SELECT 1",
            vec![TextEdit::replace(TextRange::new(7.into(), 8.into()), "2")],
        )
        .unwrap();

        assert_eq!(result, "SELECT 2");
    }

    #[test]
    fn apply_edits_handles_growing_replacements() {
        let result = apply_edits(
            "SELECT 1",
            vec![TextEdit::replace(TextRange::new(7.into(), 8.into()), "123")],
        )
        .unwrap();

        assert_eq!(result, "SELECT 123");
    }

    #[test]
    fn apply_edits_handles_shrinking_replacements() {
        let result = apply_edits(
            "SELECT 123",
            vec![TextEdit::replace(TextRange::new(7.into(), 10.into()), "1")],
        )
        .unwrap();

        assert_eq!(result, "SELECT 1");
    }

    #[test]
    fn apply_edits_handles_unicode_boundaries() {
        let text = "a😀b";
        let result =
            apply_edits(text, vec![TextEdit::replace(TextRange::new(1.into(), 5.into()), "🙂")])
                .unwrap();

        assert_eq!(result, "a🙂b");
    }

    #[test]
    fn apply_edits_rejects_invalid_unicode_boundaries() {
        let text = "a😀b";
        let result =
            apply_edits(text, vec![TextEdit::replace(TextRange::new(2.into(), 5.into()), "x")]);

        assert_eq!(result, Err(EditError::InvalidBoundary));
    }

    #[test]
    fn apply_edits_handles_insertion_at_eof() {
        let result = apply_edits("SELECT", vec![TextEdit::insert(6.into(), " 1")]).unwrap();

        assert_eq!(result, "SELECT 1");
    }

    #[test]
    fn apply_edits_handles_adjacent_edits() {
        let result = apply_edits(
            "abcd",
            vec![
                TextEdit::replace(TextRange::new(1.into(), 2.into()), "B"),
                TextEdit::replace(TextRange::new(2.into(), 3.into()), "C"),
            ],
        )
        .unwrap();

        assert_eq!(result, "aBCd");
    }

    #[test]
    fn apply_edits_rejects_overlapping_edits() {
        let result = apply_edits(
            "abcd",
            vec![
                TextEdit::replace(TextRange::new(1.into(), 3.into()), "BC"),
                TextEdit::replace(TextRange::new(2.into(), 4.into()), "CD"),
            ],
        );

        assert_eq!(result, Err(EditError::Overlap));
    }

    #[test]
    fn apply_edits_preserves_same_offset_insert_order() {
        let result = apply_edits(
            "ab",
            vec![TextEdit::insert(1.into(), "X"), TextEdit::insert(1.into(), "Y")],
        )
        .unwrap();

        assert_eq!(result, "aXYb");
    }

    #[test]
    fn edit_error_display() {
        assert_eq!(EditError::Overlap.to_string(), "edits overlap or are not strictly ordered");
        assert_eq!(
            EditError::OutOfBounds.to_string(),
            "edit refers to offsets outside the input text"
        );
        assert_eq!(EditError::InvalidBoundary.to_string(), "edit splits a UTF-8 code point");
    }
}
