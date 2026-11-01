// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::mem;

use itertools::Itertools;

use super::writer::write_nodes;
use super::FieldSearchMode;
use super::Node;
use super::SearchNode;
use super::StateKind;
use super::TemplateKind;
use crate::prelude::*;
use crate::text::escape_anki_wildcards_for_search_node;

pub trait Negated {
    fn negated(self) -> Node;
}

pub trait JoinSearches {
    /// Concatenates two sets of [Node]s, inserting [Node::And], and grouping,
    /// if appropriate.
    fn and(self, other: impl Into<SearchBuilder>) -> SearchBuilder;
    /// Concatenates two sets of [Node]s, inserting [Node::And] if appropriate,
    /// but without grouping either set.
    fn and_flat(self, other: impl Into<SearchBuilder>) -> SearchBuilder;
}

impl<T: Into<Node>> Negated for T {
    fn negated(self) -> Node {
        let node: Node = self.into();
        if let Node::Not(inner) = node {
            *inner
        } else {
            Node::Not(Box::new(node))
        }
    }
}

impl<T: Into<SearchBuilder>> JoinSearches for T {
    fn and(self, other: impl Into<SearchBuilder>) -> SearchBuilder {
        self.into().join_other(other.into(), Node::And, true)
    }

    fn and_flat(self, other: impl Into<SearchBuilder>) -> SearchBuilder {
        self.into().join_other(other.into(), Node::And, false)
    }
}

/// Helper to programmatically build searches.
#[derive(Debug, PartialEq, Clone)]
pub struct SearchBuilder(Vec<Node>);

impl SearchBuilder {
    pub fn new() -> Self {
        Self(vec![])
    }

    /// Construct [SearchBuilder] where given [Node]s are joined by [Node::And].
    pub fn all(iter: impl IntoIterator<Item = impl Into<Node>>) -> Self {
        Self(Itertools::intersperse(iter.into_iter().map(Into::into), Node::And).collect())
    }

    /// Construct [SearchBuilder] where given [Node]s are joined by [Node::Or].
    pub fn any(iter: impl IntoIterator<Item = impl Into<Node>>) -> Self {
        Self(Itertools::intersperse(iter.into_iter().map(Into::into), Node::Or).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    fn join_other(mut self, mut other: Self, joiner: Node, group: bool) -> Self {
        if group {
            self = self.group();
            other = other.group();
        }
        if !(self.is_empty() || other.is_empty()) {
            self.0.push(joiner);
        }
        self.0.append(&mut other.0);
        self
    }

    /// Wrap [Node]s in [Node::Group] if there is more than 1.
    pub fn group(mut self) -> Self {
        if self.len() > 1 {
            self.0 = vec![Node::Group(mem::take(&mut self.0))];
        }
        self
    }

    pub fn write(&self) -> String {
        write_nodes(&self.0)
    }

    /// Construct [SearchBuilder] matching learning, but not relearning cards.
    pub fn learning_cards() -> Self {
        StateKind::Learning.and(StateKind::Review.negated())
    }

    /// Construct [SearchBuilder] matching relearning cards.
    pub fn relearning_cards() -> Self {
        StateKind::Learning.and(StateKind::Review)
    }
}

impl<T: Into<Node>> From<T> for SearchBuilder {
    fn from(node: T) -> Self {
        Self(vec![node.into()])
    }
}

impl TryIntoSearch for SearchBuilder {
    fn try_into_search(self) -> Result<Node, AnkiError> {
        Ok(self.group().0.remove(0))
    }
}

impl Default for SearchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchNode {
    pub fn from_deck_id(did: impl Into<DeckId>, with_children: bool) -> Self {
        if with_children {
            Self::DeckIdWithChildren(did.into())
        } else {
            Self::DeckIdsWithoutChildren(did.into().to_string())
        }
    }

    /// Construct [SearchNode] from an unescaped deck name.
    pub fn from_deck_name(name: &str) -> Self {
        Self::Deck(escape_anki_wildcards_for_search_node(name))
    }

    /// Construct [SearchNode] from an unescaped tag name.
    pub fn from_tag_name(name: &str) -> Self {
        Self::Tag {
            tag: escape_anki_wildcards_for_search_node(name),
            mode: FieldSearchMode::Normal,
        }
    }

    pub fn from_note_ids<I: IntoIterator<Item = N>, N: Into<NoteId>>(ids: I) -> Self {
        Self::NoteIds(ids.into_iter().map(Into::into).join(","))
    }

    pub fn from_card_ids<I: IntoIterator<Item = C>, C: Into<CardId>>(ids: I) -> Self {
        Self::CardIds(ids.into_iter().map(Into::into).join(","))
    }
}

impl<T: Into<SearchNode>> From<T> for Node {
    fn from(node: T) -> Self {
        Self::Search(node.into())
    }
}

impl From<NotetypeId> for SearchNode {
    fn from(id: NotetypeId) -> Self {
        SearchNode::NotetypeId(id)
    }
}

impl From<TemplateKind> for SearchNode {
    fn from(k: TemplateKind) -> Self {
        SearchNode::CardTemplate(k)
    }
}

impl From<NoteId> for SearchNode {
    fn from(n: NoteId) -> Self {
        SearchNode::NoteIds(format!("{n}"))
    }
}

impl From<StateKind> for SearchNode {
    fn from(k: StateKind) -> Self {
        SearchNode::State(k)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn negating() {
        let node = Node::Search(SearchNode::UnqualifiedText("foo".to_string()));
        let neg_node = Node::Not(Box::new(Node::Search(SearchNode::UnqualifiedText(
            "foo".to_string(),
        ))));
        assert_eq!(node.clone().negated(), neg_node);
        assert_eq!(node.clone().negated().negated(), node);

        assert_eq!(
            StateKind::Due.negated(),
            Node::Not(Box::new(Node::Search(SearchNode::State(StateKind::Due))))
        )
    }
}
