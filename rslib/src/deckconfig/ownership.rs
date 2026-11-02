// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Each normal deck has one row of settings, with the same id as the deck, so
//! there's no way for two decks to use the same one.
//!
//! A collection from before this, or a package, has decks that point at rows
//! other decks may use too, and overrides on the deck itself. This gives every
//! deck its own row with what it was using, and removes the rest. It looks only
//! at what's there, so does nothing if all is well, and a later open repairs
//! whatever got in some other way.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::OpenFlags;

use crate::prelude::*;
use crate::storage::all_deck_config_unchecked;
use crate::storage::all_decks;

/// Values above this are reset when a row is read.
const MAX_DAILY_LIMIT: u32 = 9999;
const MIN_DESIRED_RETENTION: f32 = 0.7;
const MAX_DESIRED_RETENTION: f32 = 0.99;

#[derive(Default)]
struct Repairs {
    rows_to_write: Vec<DeckConfig>,
    decks_to_write: Vec<Deck>,
    rows_to_remove: Vec<DeckConfigId>,
}

impl Repairs {
    fn new(decks: Vec<Deck>, rows: Vec<DeckConfig>) -> Self {
        let rows: HashMap<DeckConfigId, DeckConfig> = rows.into_iter().map(|c| (c.id, c)).collect();
        let mut repairs = Self::default();
        let mut owned = vec![];
        for deck in decks {
            let Some(id) = deck.config_id() else {
                continue;
            };
            owned.push(id);
            let settings = settings_for_deck(&deck, &rows);
            if rows.get(&id) != Some(&settings) {
                repairs.rows_to_write.push(settings);
            }
            let mut repaired = deck.clone();
            repaired.keep_settings_pointer_and_clear_overrides();
            if repaired != deck {
                repairs.decks_to_write.push(repaired);
            }
        }
        repairs.rows_to_remove = rows.into_keys().filter(|id| !owned.contains(id)).collect();
        repairs
    }

    fn is_empty(&self) -> bool {
        self.rows_to_write.is_empty()
            && self.decks_to_write.is_empty()
            && self.rows_to_remove.is_empty()
    }
}

/// What the deck uses now, as its own row: values as they're stored, with its
/// overrides in place of what they overrode.
fn settings_for_deck(deck: &Deck, rows: &HashMap<DeckConfigId, DeckConfig>) -> DeckConfig {
    let source = if deck.normal().is_ok_and(|n| n.config_id == deck.id.0) {
        rows.get(&DeckConfigId(deck.id.0))
    } else {
        // A deck that points at a row which doesn't exist was using row 1.
        deck.normal()
            .ok()
            .and_then(|n| rows.get(&DeckConfigId(n.config_id)))
            .or_else(|| rows.get(&DeckConfigId(1)))
    };
    let mut settings = DeckConfig::with_inner_for_deck(
        deck.id,
        source
            .map(|c| c.inner.clone())
            .unwrap_or_else(|| DeckConfig::default().inner),
    );
    if let Some(source) = source {
        settings.mtime_secs = source.mtime_secs;
    }
    if let Ok(normal) = deck.normal() {
        if let Some(limit) = normal.review_limit {
            settings.inner.reviews_per_day = limit.min(MAX_DAILY_LIMIT);
        }
        if let Some(limit) = normal.new_limit {
            settings.inner.new_per_day = limit.min(MAX_DAILY_LIMIT);
        }
        if let Some(retention) = normal.desired_retention {
            settings.inner.desired_retention =
                retention.clamp(MIN_DESIRED_RETENTION, MAX_DESIRED_RETENTION);
        }
    }
    settings
}

/// Whether opening this closed collection would change its settings, or
/// whether that can't be told. Opens it read-only. For taking a backup first.
pub fn settings_repair_needed(collection: &Path) -> Result<bool> {
    let db = Connection::open_with_flags(collection, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(!Repairs::new(all_decks(&db)?, all_deck_config_unchecked(&db)?).is_empty())
}

impl Collection {
    /// To be called on opening a collection. All of it happens in one
    /// transaction, so an interruption leaves things as they were, and the
    /// next open starts again.
    pub(crate) fn ensure_decks_own_settings(&mut self) -> Result<()> {
        if self.settings_repairs()?.is_empty() {
            return Ok(());
        }
        self.transact_no_undo(|col| col.ensure_decks_own_settings_inner())
    }

    /// Caller must set up transaction.
    pub(crate) fn ensure_decks_own_settings_inner(&mut self) -> Result<()> {
        let repairs = self.settings_repairs()?;
        for id in &repairs.rows_to_remove {
            self.storage.remove_deck_conf(*id)?;
        }
        for row in &repairs.rows_to_write {
            self.storage
                .add_or_update_deck_config_with_existing_id(row)?;
        }
        for deck in &repairs.decks_to_write {
            self.storage.update_deck(deck)?;
        }
        self.state.deck_cache.clear();
        Ok(())
    }

    fn settings_repairs(&self) -> Result<Repairs> {
        Ok(Repairs::new(
            self.storage.get_all_decks()?,
            self.storage.all_deck_config_unchecked()?,
        ))
    }

    /// A deck that's just arrived has built-in values, unless it had a row.
    pub(crate) fn add_settings_if_missing(&mut self, deck: &Deck) -> Result<()> {
        if deck.is_normal() {
            let mut config = DeckConfig::new_for_deck(deck.id);
            config.set_modified();
            self.add_deck_config_if_unique_undoable(&config)?;
        }
        Ok(())
    }

    /// Back to built-in values, as for a new deck.
    pub(crate) fn reset_settings(&mut self, deck: DeckId) -> Result<()> {
        let mut config = DeckConfig::new_for_deck(deck);
        self.add_or_update_deck_config(&mut config)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use anki_proto::decks::deck::normal::DayLimit;

    use super::*;
    use crate::collection::CollectionBuilder;
    use crate::deckconfig::DeckConfigInner;
    use crate::decks::NormalDeck;
    use crate::tests::CardAdder;
    use crate::tests::DeckAdder;
    use crate::tests::NoteAdder;

    fn add_row(col: &Collection, id: i64, name: &str, modify: impl FnOnce(&mut DeckConfigInner)) {
        let mut row = DeckConfig {
            id: DeckConfigId(id),
            name: name.into(),
            mtime_secs: TimestampSecs(1000 + id),
            ..Default::default()
        };
        modify(&mut row.inner);
        col.storage
            .add_or_update_deck_config_with_existing_id(&row)
            .unwrap();
    }

    /// As an older version would have left it, bypassing what keeps decks
    /// apart.
    fn point_deck_at(
        col: &mut Collection,
        deck: DeckId,
        row: i64,
        modify: impl FnOnce(&mut NormalDeck),
    ) {
        let mut deck = col.storage.get_deck(deck).unwrap().unwrap();
        let normal = deck.normal_mut().unwrap();
        normal.config_id = row;
        modify(normal);
        col.storage.update_deck(&deck).unwrap();
        col.state.deck_cache.clear();
    }

    /// What a deck used, by the rules from before this.
    fn effective_before(col: &Collection) -> HashMap<DeckId, DeckConfigInner> {
        let rows: HashMap<_, _> = col
            .storage
            .all_deck_config_unchecked()
            .unwrap()
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        col.storage
            .get_all_decks()
            .unwrap()
            .into_iter()
            .filter(Deck::is_normal)
            .map(|deck| {
                let normal = deck.normal().unwrap();
                let mut inner = rows
                    .get(&DeckConfigId(normal.config_id))
                    .or_else(|| rows.get(&DeckConfigId(1)))
                    .unwrap()
                    .inner
                    .clone();
                if let Some(limit) = normal.review_limit {
                    inner.reviews_per_day = limit;
                }
                if let Some(limit) = normal.new_limit {
                    inner.new_per_day = limit;
                }
                if let Some(retention) = normal.desired_retention {
                    inner.desired_retention = retention;
                }
                (deck.id, inner)
            })
            .collect()
    }

    fn effective_after(col: &Collection) -> HashMap<DeckId, DeckConfigInner> {
        col.storage
            .get_all_decks()
            .unwrap()
            .into_iter()
            .filter(Deck::is_normal)
            .map(|deck| {
                let row = col
                    .storage
                    .get_deck_config(deck.config_id().unwrap())
                    .unwrap()
                    .unwrap();
                (deck.id, row.inner)
            })
            .collect()
    }

    fn assert_one_row_per_normal_deck(col: &Collection) {
        let decks: HashSet<_> = col
            .storage
            .get_all_decks()
            .unwrap()
            .iter()
            .filter_map(Deck::config_id)
            .collect();
        let rows: HashSet<_> = col
            .storage
            .all_deck_config()
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(decks, rows);
        for deck in col.storage.get_all_decks().unwrap() {
            if let Ok(normal) = deck.normal() {
                assert_eq!(normal.config_id, deck.id.0);
                assert_eq!(normal.review_limit, None);
                assert_eq!(normal.new_limit, None);
                assert_eq!(normal.desired_retention, None);
            }
        }
    }

    /// Everything about the cards, notes and history, as stored.
    fn study_data(col: &Collection) -> Vec<String> {
        let mut out = vec![];
        for (table, columns) in [
            ("cards", "id,nid,did,ord,mod,type,queue,due,ivl,factor,reps,lapses,left,odue,odid,flags,data"),
            ("notes", "id,guid,mid,mod,tags,flds,sfld,csum,flags,data"),
            ("revlog", "id,cid,ease,ivl,lastIvl,factor,time,type"),
        ] {
            let db = col.storage.db();
            let sql = format!("select {columns} from {table} order by id");
            let mut statement = db.prepare(&sql).unwrap();
            let count = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    let values: Vec<String> = (0..count)
                        .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                        .collect();
                    Ok(values.join("|"))
                })
                .unwrap();
            out.extend(rows.map(|row| row.unwrap()));
        }
        out
    }

    /// Like the real collection this was written for.
    fn legacy_collection() -> (Collection, HashMap<&'static str, DeckId>) {
        let mut col = Collection::new();
        let mut decks = HashMap::new();
        for name in [
            "Geography",
            "Citizenship",
            "Yomitan",
            "Kaishi",
            "Info",
            "Parent::Child",
        ] {
            decks.insert(name, DeckAdder::new(name).add(&mut col).id);
        }
        decks.insert("Parent", col.get_deck_id("Parent").unwrap().unwrap());
        decks.insert("Default", DeckId(1));

        add_row(&col, 1, "Default", |c| {
            c.reviews_per_day = 500;
            c.fsrs_params_5 = vec![0.5; 19];
            c.desired_retention = 0.9;
        });
        add_row(&col, 1634166536617, "Unused", |c| c.new_per_day = 1);
        add_row(&col, 1742871112974, "Life", |c| c.new_per_day = 2);
        add_row(&col, 1742871113067, "Life", |c| {
            c.new_per_day = 10;
            c.fsrs_params_5 = vec![0.7; 19];
        });
        add_row(&col, 1742972850732, "Japanese", |c| {
            c.learn_steps = vec![];
            c.relearn_steps = vec![];
            c.new_per_day = 20;
            c.reviews_per_day = 300;
            c.desired_retention = 0.91;
            c.historical_retention = 0.85;
            c.fsrs_params_5 = vec![0.9; 19];
            c.other = b"{}".to_vec();
        });
        add_row(
            &col,
            decks["Parent"].0,
            "Same id as a deck, used by another",
            |c| {
                c.new_per_day = 3;
            },
        );

        point_deck_at(&mut col, decks["Geography"], 1, |_| ());
        point_deck_at(&mut col, decks["Citizenship"], 1, |_| ());
        point_deck_at(&mut col, decks["Yomitan"], 1742972850732, |_| ());
        point_deck_at(&mut col, decks["Kaishi"], 1742972850732, |n| {
            n.new_limit = Some(15);
            n.review_limit = Some(250);
            n.desired_retention = Some(0.93);
            n.new_limit_today = Some(DayLimit { limit: 4, today: 3 });
        });
        point_deck_at(&mut col, decks["Info"], 1742871113067, |_| ());
        // a row that happens to have the id of a deck that doesn't use it
        point_deck_at(&mut col, decks["Parent"], 1, |_| ());
        point_deck_at(&mut col, decks["Parent::Child"], decks["Parent"].0, |_| ());

        for deck in decks.values() {
            NoteAdder::basic(&mut col).deck(*deck).add(&mut col);
        }
        (col, decks)
    }

    #[test]
    fn every_deck_ends_up_with_exactly_what_it_used() {
        let (mut col, decks) = legacy_collection();
        let expected = effective_before(&col);
        let study_before = study_data(&col);
        assert_eq!(expected.len(), decks.len());

        col.ensure_decks_own_settings().unwrap();

        assert_eq!(effective_after(&col), expected);
        assert_eq!(study_data(&col), study_before);
        assert_one_row_per_normal_deck(&col);

        let after = effective_after(&col);
        assert_eq!(after[&decks["Kaishi"]].new_per_day, 15);
        assert_eq!(after[&decks["Kaishi"]].reviews_per_day, 250);
        assert_eq!(after[&decks["Kaishi"]].desired_retention, 0.93);
        assert_eq!(after[&decks["Yomitan"]].new_per_day, 20);
        assert!(after[&decks["Yomitan"]].learn_steps.is_empty());
        assert_eq!(after[&decks["Info"]].fsrs_params_5, vec![0.7; 19]);
        assert_eq!(after[&decks["Parent::Child"]].new_per_day, 3);
        assert_eq!(after[&decks["Default"]].reviews_per_day, 500);

        let kaishi = col.storage.get_deck(decks["Kaishi"]).unwrap().unwrap();
        assert_eq!(
            kaishi.normal().unwrap().new_limit_today,
            Some(DayLimit { limit: 4, today: 3 })
        );

        // the row's own history comes along
        let info = col
            .get_deck_config(DeckConfigId(decks["Info"].0), false)
            .unwrap();
        assert_eq!(
            info.unwrap().mtime_secs,
            TimestampSecs(1000 + 1742871113067)
        );
    }

    #[test]
    fn a_deck_pointing_at_nothing_keeps_what_it_was_getting() {
        let (mut col, decks) = legacy_collection();
        point_deck_at(&mut col, decks["Info"], 987654321, |_| ());
        let expected = effective_before(&col);
        col.ensure_decks_own_settings().unwrap();
        assert_eq!(effective_after(&col), expected);
        assert_eq!(effective_after(&col)[&decks["Info"]].reviews_per_day, 500);
    }

    #[test]
    fn deck_list_counts_use_the_deck_only_limit_after_migrating() {
        let mut col = Collection::new();
        let shared = DeckAdder::new("shared").add(&mut col);
        let limited = DeckAdder::new("limited").add(&mut col);
        add_row(&col, 42, "Shared", |c| c.new_per_day = 5);
        point_deck_at(&mut col, shared.id, 42, |_| ());
        point_deck_at(&mut col, limited.id, 42, |n| n.new_limit = Some(2));
        for deck in [shared.id, limited.id] {
            for _ in 0..8 {
                NoteAdder::basic(&mut col).deck(deck).add(&mut col);
            }
        }

        col.ensure_decks_own_settings().unwrap();
        let tree = col.deck_tree(Some(TimestampSecs::now())).unwrap();
        let new_count = |name: &str| {
            tree.children
                .iter()
                .find(|n| n.name == name)
                .unwrap()
                .new_count
        };
        assert_eq!(new_count("shared"), 5);
        assert_eq!(new_count("limited"), 2);
    }

    #[test]
    fn running_again_changes_nothing() {
        let (mut col, _) = legacy_collection();
        col.ensure_decks_own_settings().unwrap();
        let rows = col.storage.all_deck_config_unchecked().unwrap();
        let decks = col.storage.get_all_decks().unwrap();
        let changed = col.storage.get_collection_timestamps().unwrap();

        assert!(col.settings_repairs().unwrap().is_empty());
        col.ensure_decks_own_settings().unwrap();

        assert_eq!(col.storage.all_deck_config_unchecked().unwrap(), rows);
        assert_eq!(col.storage.get_all_decks().unwrap(), decks);
        assert_eq!(
            col.storage
                .get_collection_timestamps()
                .unwrap()
                .collection_change,
            changed.collection_change
        );
    }

    #[test]
    fn an_interrupted_run_leaves_everything_as_it_was() {
        let (mut col, _) = legacy_collection();
        let rows = col.storage.all_deck_config_unchecked().unwrap();
        let decks = col.storage.get_all_decks().unwrap();

        let failed: Result<()> = col.transact_no_undo(|col| {
            col.ensure_decks_own_settings_inner()?;
            assert_one_row_per_normal_deck(col);
            invalid_input!("interrupted");
        });
        assert!(failed.is_err());

        assert_eq!(col.storage.all_deck_config_unchecked().unwrap(), rows);
        assert_eq!(col.storage.get_all_decks().unwrap(), decks);
        // and the next attempt works
        let expected = effective_before(&col);
        col.ensure_decks_own_settings().unwrap();
        assert_eq!(effective_after(&col), expected);
    }

    #[test]
    fn sharing_that_gets_in_later_is_repaired_on_the_next_open() {
        let mut col = Collection::new();
        let first = DeckAdder::new("first")
            .with_config(|c| c.inner.new_per_day = 11)
            .add(&mut col);
        let second = DeckAdder::new("second")
            .with_config(|c| c.inner.new_per_day = 22)
            .add(&mut col);
        // as an import or a rollback to an old build could
        point_deck_at(&mut col, second.id, first.id.0, |_| ());
        add_row(&col, 555, "Leftover", |_| ());

        col.ensure_decks_own_settings().unwrap();

        assert_one_row_per_normal_deck(&col);
        assert_eq!(effective_after(&col)[&second.id].new_per_day, 11);
    }

    #[test]
    fn decks_cant_be_made_to_share() {
        let mut col = Collection::new();
        let first = DeckAdder::new("first").add(&mut col);
        let mut second = DeckAdder::new("second").add(&mut col);

        // the id of a deck's settings is its own, whatever the stored value
        second.normal_mut().unwrap().config_id = first.id.0;
        assert_ne!(second.config_id(), first.config_id());
        col.update_deck(&mut second).unwrap();
        let saved = col.storage.get_deck(second.id).unwrap().unwrap();
        assert_eq!(saved.normal().unwrap().config_id, second.id.0);
        assert_one_row_per_normal_deck(&col);
    }

    #[test]
    fn editing_one_decks_settings_leaves_the_others() {
        let (mut col, decks) = legacy_collection();
        col.ensure_decks_own_settings().unwrap();
        let before = effective_after(&col);

        let mut kaishi = col
            .get_deck_config(DeckConfigId(decks["Kaishi"].0), false)
            .unwrap()
            .unwrap();
        kaishi.inner.new_per_day = 1;
        col.add_or_update_deck_config(&mut kaishi).unwrap();

        let after = effective_after(&col);
        assert_eq!(after[&decks["Kaishi"]].new_per_day, 1);
        assert_eq!(after[&decks["Yomitan"]], before[&decks["Yomitan"]]);
        assert_eq!(after[&decks["Default"]], before[&decks["Default"]]);
    }

    #[test]
    fn a_new_collection_needs_nothing_and_its_row_has_no_name_to_show() {
        let mut col = Collection::new();
        assert_one_row_per_normal_deck(&col);
        assert!(col.settings_repairs().unwrap().is_empty());
        let row = col
            .get_deck_config(DeckConfigId(1), false)
            .unwrap()
            .unwrap();
        assert_eq!(row.name, "1");
        assert_eq!(row.inner, DeckConfig::default().inner);
        col.ensure_decks_own_settings().unwrap();
    }

    #[test]
    fn a_new_deck_has_built_in_values_and_its_own_row() {
        let mut col = Collection::new();
        col.update_default_deck_config(|c| c.new_per_day = 77);

        let deck = DeckAdder::new("one::two::three").add(&mut col);

        assert_one_row_per_normal_deck(&col);
        let row = col
            .get_deck_config(deck.config_id().unwrap(), false)
            .unwrap();
        assert_eq!(row.unwrap().inner, DeckConfig::default().inner);
        // parents made along the way too
        let parent = col.get_deck_id("one::two").unwrap().unwrap();
        let row = col.get_deck_config(DeckConfigId(parent.0), false).unwrap();
        assert_eq!(row.unwrap().inner, DeckConfig::default().inner);

        // and undoing removes the row with the deck
        let rows_before = col.storage.all_deck_config().unwrap().len();
        let mut another = Deck::new_normal();
        another.name = NativeDeckName::from_human_name("another");
        col.add_deck(&mut another).unwrap();
        assert_eq!(
            col.storage.all_deck_config().unwrap().len(),
            rows_before + 1
        );
        col.undo().unwrap();
        assert_eq!(col.storage.all_deck_config().unwrap().len(), rows_before);
    }

    #[test]
    fn a_filtered_deck_has_no_row() {
        let mut col = Collection::new();
        let filtered = DeckAdder::new("filtered").filtered(true).add(&mut col);
        assert_eq!(filtered.config_id(), None);
        assert_one_row_per_normal_deck(&col);
        assert!(col
            .storage
            .get_deck_config(DeckConfigId(filtered.id.0))
            .unwrap()
            .is_none());
    }

    #[test]
    fn deleting_a_deck_deletes_its_row_and_undo_brings_it_back() {
        let mut col = Collection::new();
        let parent = DeckAdder::new("parent")
            .with_config(|c| c.inner.new_per_day = 3)
            .add(&mut col);
        let child = DeckAdder::new("parent::child").add(&mut col);

        col.remove_decks_and_child_decks(&[parent.id]).unwrap();

        assert_one_row_per_normal_deck(&col);
        for deck in [&parent, &child] {
            assert!(col
                .storage
                .get_deck_config(deck.config_id().unwrap())
                .unwrap()
                .is_none());
        }

        col.undo().unwrap();
        assert_one_row_per_normal_deck(&col);
        let row = col
            .get_deck_config(parent.config_id().unwrap(), false)
            .unwrap();
        assert_eq!(row.unwrap().inner.new_per_day, 3);
    }

    #[test]
    fn deleting_the_default_deck_leaves_it_with_fresh_settings() {
        let mut col = Collection::new();
        col.update_default_deck_config(|c| c.new_per_day = 77);
        CardAdder::new().deck(DeckId(1)).add(&mut col);

        col.remove_decks_and_child_decks(&[DeckId(1)]).unwrap();

        assert!(col.storage.get_deck(DeckId(1)).unwrap().is_some());
        assert_one_row_per_normal_deck(&col);
        let row = col
            .get_deck_config(DeckConfigId(1), false)
            .unwrap()
            .unwrap();
        assert_eq!(
            row.inner.new_per_day,
            DeckConfig::default().inner.new_per_day
        );
    }

    #[test]
    fn renaming_a_deck_keeps_its_row() {
        let mut col = Collection::new();
        let deck = DeckAdder::new("before")
            .with_config(|c| c.inner.new_per_day = 3)
            .add(&mut col);
        col.rename_deck(deck.id, "after").unwrap();
        assert_one_row_per_normal_deck(&col);
        let row = col
            .get_deck_config(deck.config_id().unwrap(), false)
            .unwrap();
        assert_eq!(row.unwrap().inner.new_per_day, 3);
    }

    #[test]
    fn a_database_check_repairs_too() {
        let (mut col, _) = legacy_collection();
        let expected = effective_before(&col);
        col.check_database().unwrap();
        assert_one_row_per_normal_deck(&col);
        assert_eq!(effective_after(&col), expected);
    }

    #[test]
    fn overrides_past_what_a_row_can_hold_are_capped_rather_than_reset() {
        let mut col = Collection::new();
        let deck = DeckAdder::new("deck").add(&mut col);
        point_deck_at(&mut col, deck.id, deck.id.0, |n| {
            n.new_limit = Some(100_000);
            n.desired_retention = Some(0.995);
        });
        col.ensure_decks_own_settings().unwrap();
        let row = col
            .get_deck_config(deck.config_id().unwrap(), false)
            .unwrap()
            .unwrap();
        assert_eq!(row.inner.new_per_day, MAX_DAILY_LIMIT);
        assert_eq!(row.inner.desired_retention, MAX_DESIRED_RETENTION);
    }

    #[test]
    fn settings_repair_needed_reads_a_closed_file() {
        let (mut col, tempdir) = crate::tests::open_fs_test_collection("needs");
        let path = col.col_path.clone();
        col.storage.db().execute_batch("pragma wal_checkpoint").ok();
        add_row(&col, 42, "Shared", |_| ());
        let default = DeckId(1);
        point_deck_at(&mut col, default, 42, |_| ());
        col.close().unwrap();

        assert!(settings_repair_needed(&path).unwrap());

        let mut col = CollectionBuilder::new(&path).build().unwrap();
        col.ensure_decks_own_settings().unwrap();
        col.close().unwrap();

        assert!(!settings_repair_needed(&path).unwrap());
        drop(tempdir);
    }
}
