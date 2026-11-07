// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    AddDeck,
    AddNote,
    AddNotetype,
    AnswerCard,
    BuildFilteredDeck,
    CreateCustomStudy,
    EmptyCards,
    EmptyFilteredDeck,
    Import,
    RebuildFilteredDeck,
    RemoveDeck,
    RemoveNote,
    RemoveTag,
    RenameDeck,
    RenameTag,
    SetCardDeck,
    Suspend,
    Unsuspend,
    UpdateConfig,
    UpdateDeck,
    UpdateDeckConfig,
    UpdateNote,
    UpdateTag,
    UpdateNotetype,
    SetCurrentDeck,
    /// Does not register changes in undo queue, but does not clear the current
    /// queue either.
    SkipUndo,
}

impl Op {
    pub fn describe(&self, tr: &I18n) -> String {
        match self {
            Op::AddDeck => tr.actions_add_deck(),
            Op::AddNote => tr.actions_add_note(),
            Op::AnswerCard => tr.actions_answer_card(),
            Op::CreateCustomStudy => tr.actions_custom_study(),
            Op::EmptyCards => tr.actions_empty_cards(),
            Op::Import => tr.actions_import(),
            Op::RemoveDeck => tr.decks_delete_deck(),
            Op::RemoveNote => tr.studying_delete_note(),
            Op::RenameDeck => tr.actions_rename_deck(),
            Op::Suspend => tr.studying_suspend(),
            Op::Unsuspend => tr.actions_unsuspend(),
            Op::UpdateConfig => tr.actions_update_config(),
            Op::UpdateDeck => tr.actions_update_deck(),
            Op::UpdateNote => tr.actions_update_note(),
            Op::UpdateTag => tr.actions_update_tag(),
            Op::SetCardDeck => tr.browsing_change_deck(),
            Op::RenameTag => tr.actions_rename_tag(),
            Op::RemoveTag => tr.actions_remove_tag(),
            Op::BuildFilteredDeck => tr.actions_build_filtered_deck(),
            Op::RebuildFilteredDeck => tr.actions_build_filtered_deck(),
            Op::EmptyFilteredDeck => tr.studying_empty(),
            Op::SetCurrentDeck => tr.browsing_select_deck(),
            Op::UpdateDeckConfig => tr.deck_config_title(),
            Op::AddNotetype => tr.actions_add_notetype(),
            Op::UpdateNotetype => tr.actions_update_notetype(),
            Op::SkipUndo => return "".to_string(),
        }
        .into()
    }
}

#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
pub struct StateChanges {
    pub card: bool,
    pub note: bool,
    pub deck: bool,
    pub tag: bool,
    pub notetype: bool,
    pub config: bool,
    pub deck_config: bool,
    pub mtime: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OpChanges {
    pub op: Op,
    pub changes: StateChanges,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpOutput<T> {
    pub output: T,
    pub changes: OpChanges,
}

impl<T> OpOutput<T> {
    pub(crate) fn map<F, N>(self, func: F) -> OpOutput<N>
    where
        F: FnOnce(T) -> N,
    {
        OpOutput {
            output: func(self.output),
            changes: self.changes,
        }
    }
}

impl OpChanges {
    #[cfg(test)]
    pub fn had_change(&self) -> bool {
        let c = &self.changes;
        c.card || c.config || c.deck || c.deck_config || c.note || c.notetype || c.tag || c.mtime
    }

    pub fn requires_study_queue_rebuild(&self) -> bool {
        let c = &self.changes;
        c.card
            || c.deck
            || (c.config && matches!(self.op, Op::SetCurrentDeck | Op::UpdateDeckConfig))
            || c.deck_config
    }
}
