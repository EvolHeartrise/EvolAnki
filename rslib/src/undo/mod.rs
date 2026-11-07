// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod changes;

use std::collections::VecDeque;

pub(crate) use changes::UndoableChange;

pub use crate::ops::Op;
use crate::ops::OpChanges;
use crate::ops::StateChanges;
use crate::prelude::*;

const UNDO_LIMIT: usize = 30;

#[derive(Debug)]
pub(crate) struct UndoableOp {
    pub kind: Op,
    pub timestamp: TimestampSecs,
    pub changes: Vec<UndoableChange>,
}

impl UndoableOp {
    /// True if changes non-empty.
    fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
}

pub struct UndoStatus {
    pub undo: Option<Op>,
}

pub struct UndoOutput {
    pub undone_op: Op,
}

#[derive(Debug, Default)]
pub(crate) struct UndoManager {
    // undo steps are added to the front of a double-ended queue, so we can
    // efficiently cap the number of steps we retain in memory
    undo_steps: VecDeque<UndoableOp>,
    undoing: bool,
    current_step: Option<UndoableOp>,
}

impl UndoManager {
    fn save(&mut self, item: UndoableChange) {
        if let Some(step) = self.current_step.as_mut() {
            step.changes.push(item)
        }
    }

    fn begin_step(&mut self, op: Option<Op>) {
        if op.is_none() {
            self.undo_steps.clear();
        }
        self.current_step = op.map(|op| UndoableOp {
            kind: op,
            timestamp: TimestampSecs::now(),
            changes: vec![],
        });
    }

    fn end_step(&mut self, skip_undo: bool) {
        if let Some(step) = self.current_step.take() {
            // undoing a step isn't itself undoable
            if step.has_changes() && !skip_undo && !self.undoing {
                self.undo_steps.truncate(UNDO_LIMIT - 1);
                self.undo_steps.push_front(step);
            }
        }
    }

    fn can_undo(&self) -> Option<&Op> {
        self.undo_steps.front().map(|s| &s.kind)
    }

    fn previous_op(&self) -> Option<&UndoableOp> {
        self.undo_steps.front()
    }

    fn current_op(&self) -> Option<&UndoableOp> {
        self.current_step.as_ref()
    }

    fn op_changes(&self) -> OpChanges {
        let current_op = self
            .current_step
            .as_ref()
            .expect("current_changes() called when no op set");

        let changes = StateChanges::from(&current_op.changes[..]);
        OpChanges {
            op: current_op.kind.clone(),
            changes,
        }
    }
}

impl Collection {
    pub fn can_undo(&self) -> Option<&Op> {
        self.state.undo.can_undo()
    }

    pub fn undo(&mut self) -> Result<OpOutput<UndoOutput>> {
        if let Some(step) = self.state.undo.undo_steps.pop_front() {
            self.undo_inner(step)
        } else {
            Err(AnkiError::UndoEmpty)
        }
    }

    pub fn undo_status(&self) -> UndoStatus {
        UndoStatus {
            undo: self.can_undo().cloned(),
        }
    }
}

impl Collection {
    /// If op is None, clears the undo queue.
    pub(crate) fn begin_undoable_operation(&mut self, op: Option<Op>) {
        self.state.undo.begin_step(op);
    }

    /// Called at the end of a successful transaction.
    /// In most instances, this will also clear the study queues.
    pub(crate) fn end_undoable_operation(&mut self, skip_undo: bool) {
        self.state.undo.end_step(skip_undo);
    }

    pub(crate) fn discard_undo_and_study_queues(&mut self) {
        self.state.undo.begin_step(None);
        self.clear_study_queues();
    }

    #[inline]
    pub(crate) fn save_undo(&mut self, item: impl Into<UndoableChange>) {
        self.state.undo.save(item.into());
    }

    pub(crate) fn current_undo_op(&self) -> Option<&UndoableOp> {
        self.state.undo.current_op()
    }

    pub(crate) fn previous_undo_op(&self) -> Option<&UndoableOp> {
        self.state.undo.previous_op()
    }

    pub(crate) fn undoing(&self) -> bool {
        self.state.undo.undoing
    }

    pub(crate) fn current_undo_step_has_changes(&self) -> bool {
        self.state
            .undo
            .current_op()
            .map(|op| op.has_changes())
            .unwrap_or_default()
    }

    /// Used for coalescing successive note updates.
    pub(crate) fn clear_last_op(&mut self) {
        self.state
            .undo
            .current_step
            .as_mut()
            .expect("no operation active")
            .changes
            .clear()
    }

    /// Return changes made by the current op. Must only be called in a
    /// transaction, when an operation was passed to transact().
    pub(crate) fn op_changes(&self) -> OpChanges {
        self.state.undo.op_changes()
    }

    fn undo_inner(&mut self, step: UndoableOp) -> Result<OpOutput<UndoOutput>> {
        let undone_op = step.kind;
        let changes = step.changes;
        self.state.undo.undoing = true;
        let res = self.transact(undone_op.clone(), |col| {
            for change in changes.into_iter().rev() {
                change.undo(col)?;
            }
            Ok(UndoOutput { undone_op })
        });
        self.state.undo.undoing = false;
        res
    }
}

impl From<&[UndoableChange]> for StateChanges {
    fn from(changes: &[UndoableChange]) -> Self {
        let mut out = StateChanges::default();
        if !changes.is_empty() {
            out.mtime = true;
        }
        for change in changes {
            match change {
                UndoableChange::Card(_) => out.card = true,
                UndoableChange::Note(_) => out.note = true,
                UndoableChange::Deck(_) => out.deck = true,
                UndoableChange::Tag(_) => out.tag = true,
                UndoableChange::Revlog(_) => {}
                UndoableChange::Queue(_) => {}
                UndoableChange::Config(_) => out.config = true,
                UndoableChange::DeckConfig(_) => out.deck_config = true,
                UndoableChange::Collection(_) => {}
                UndoableChange::Notetype(_) => out.notetype = true,
            }
        }
        out
    }
}

#[cfg(test)]
mod test {

    use crate::card::Card;
    use crate::prelude::*;

    #[test]
    fn undo() -> Result<()> {
        let mut col = Collection::new();

        let mut card = Card {
            interval: 1,
            ..Default::default()
        };
        col.add_card(&mut card).unwrap();
        let cid = card.id;

        assert_eq!(col.can_undo(), None);

        // outside of a transaction, no undo info recorded
        let card = col
            .get_and_update_card(cid, |card| {
                card.interval = 2;
                Ok(())
            })
            .unwrap();
        assert_eq!(card.interval, 2);
        assert_eq!(col.can_undo(), None);

        // record a few undo steps
        for i in 3..=4 {
            col.transact(Op::UpdateNote, |col| {
                col.get_and_update_card(cid, |card| {
                    card.interval = i;
                    Ok(())
                })
                .unwrap();
                Ok(())
            })
            .unwrap();
        }

        assert_eq!(col.storage.get_card(cid).unwrap().unwrap().interval, 4);
        assert_eq!(col.can_undo(), Some(&Op::UpdateNote));

        // undo a step
        col.undo().unwrap();
        assert_eq!(col.storage.get_card(cid).unwrap().unwrap().interval, 3);
        assert_eq!(col.can_undo(), Some(&Op::UpdateNote));

        // and again
        col.undo().unwrap();
        assert_eq!(col.storage.get_card(cid).unwrap().unwrap().interval, 2);
        assert_eq!(col.can_undo(), None);

        col.transact(Op::UpdateNote, |col| {
            col.get_and_update_card(cid, |card| {
                card.interval = 5;
                Ok(())
            })
        })?;
        assert_eq!(col.can_undo(), Some(&Op::UpdateNote));

        // and any action that doesn't support undoing will clear the queue
        col.transact_no_undo(|_col| Ok(())).unwrap();
        assert_eq!(col.can_undo(), None);

        // if an object is mutated multiple times in one operation,
        // the changes should be undone in the correct order
        col.transact(Op::UpdateNote, |col| {
            col.get_and_update_card(cid, |card| {
                card.interval = 10;
                Ok(())
            })?;
            col.get_and_update_card(cid, |card| {
                card.interval = 15;
                Ok(())
            })
        })?;

        assert_eq!(col.storage.get_card(cid).unwrap().unwrap().interval, 15);
        col.undo()?;
        assert_eq!(col.storage.get_card(cid).unwrap().unwrap().interval, 5);

        Ok(())
    }

    #[test]
    fn undo_mtime_bump() -> Result<()> {
        let mut col = Collection::new();
        col.storage.db.execute_batch("update col set mod = 0")?;

        // a no-op change should not bump mtime
        let out = col.set_config_bool(BoolKey::AddingDefaultsToCurrentDeck, true, true)?;
        assert_eq!(
            col.storage.get_collection_timestamps()?.collection_change.0,
            0
        );
        assert!(!out.changes.had_change());

        // if there is an undoable step, mtime should change
        let out = col.set_config_bool(BoolKey::AddingDefaultsToCurrentDeck, false, true)?;
        assert_ne!(
            col.storage.get_collection_timestamps()?.collection_change.0,
            0
        );
        assert!(out.changes.had_change());

        // when skipping undo, mtime should still only be bumped on a change
        col.storage.db.execute_batch("update col set mod = 0")?;
        let out = col.set_config_bool(BoolKey::AddingDefaultsToCurrentDeck, false, false)?;
        assert_eq!(
            col.storage.get_collection_timestamps()?.collection_change.0,
            0
        );
        assert!(!out.changes.had_change());

        // op output will reflect changes were made
        let out = col.set_config_bool(BoolKey::AddingDefaultsToCurrentDeck, true, false)?;
        assert_ne!(
            col.storage.get_collection_timestamps()?.collection_change.0,
            0
        );
        assert!(out.changes.had_change());

        Ok(())
    }

    #[test]
    fn coalesce_note_undo_entries() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        note.set_field(0, "foo")?;
        col.update_note(&mut note)?;
        note.set_field(0, "bar")?;
        col.update_note(&mut note)?;
        assert_eq!(col.state.undo.undo_steps.len(), 2);

        Ok(())
    }
}
