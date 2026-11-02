// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hasher;
use std::path::Path;
use std::sync::Arc;

use bitflags::bitflags;
use fnv::FnvHasher;
use fsrs::FSRS5_DEFAULT_DECAY;
use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::params;
use rusqlite::trace::TraceEvent;
use rusqlite::Connection;
use serde_json::Value;
use unicase::UniCase;

use super::upgrades::SCHEMA_MAX_VERSION;
use super::upgrades::SCHEMA_MIN_VERSION;
use super::upgrades::SCHEMA_STARTING_VERSION;
use super::QUEUE_ONCE_OPENED_SQL;
use crate::cloze::strip_clozes;
use crate::config::schema11::schema11_config_as_string;
use crate::error::DbErrorKind;
use crate::prelude::*;
use crate::scheduler::timing::local_minutes_west_for_stamp;
use crate::scheduler::timing::v1_creation_date;
use crate::storage::card::data::CardData;
use crate::text::without_combining;
use crate::text::CowMapping;

fn unicase_compare(s1: &str, s2: &str) -> Ordering {
    UniCase::new(s1).cmp(&UniCase::new(s2))
}

// fixme: rollback savepoint when tags not changed
// fixme: need to drop out of wal prior to vacuuming to fix page size of older
// collections

#[derive(Debug)]
pub struct SqliteStorage {
    pub(crate) db: Connection,
}

fn open_or_create_collection_db(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;

    if std::env::var("TRACESQL").is_ok() {
        db.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace),
        );
    }

    db.busy_timeout(std::time::Duration::from_secs(0))?;

    db.pragma_update(None, "locking_mode", "exclusive")?;
    db.pragma_update(None, "page_size", 4096)?;
    db.pragma_update(None, "cache_size", -40 * 1024)?;
    db.pragma_update(None, "legacy_file_format", false)?;
    db.pragma_update(None, "journal_mode", "wal")?;
    // Android has no /tmp folder, and fails in the default config.
    #[cfg(target_os = "android")]
    db.pragma_update(None, "temp_store", &"memory")?;

    db.set_prepared_statement_cache_capacity(50);

    add_field_index_function(&db)?;
    add_regexp_function(&db)?;
    add_regexp_fields_function(&db)?;
    add_regexp_tags_function(&db)?;
    add_process_text_function(&db)?;
    add_fnvhash_function(&db)?;
    add_extract_original_position_function(&db)?;
    add_extract_custom_data_function(&db)?;
    add_extract_fsrs_variable(&db)?;
    add_extract_fsrs_retrievability(&db)?;
    add_extract_fsrs_relative_retrievability(&db)?;

    db.create_collation("unicase", unicase_compare)?;

    Ok(db)
}

impl SqliteStorage {
    /// This is provided as an escape hatch for when you need to do something
    /// not directly supported by this library. Please exercise caution when
    /// using it.
    pub fn db(&self) -> &Connection {
        &self.db
    }
}
/// Adds sql function field_at_index(flds, index)
/// to split provided fields and return field at zero-based index.
/// If out of range, returns empty string.
fn add_field_index_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "field_at_index",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let mut fields = ctx.get_raw(0).as_str()?.split('\x1f');
            let idx: u16 = ctx.get(1)?;
            Ok(fields.nth(idx as usize).unwrap_or("").to_string())
        },
    )
}

bitflags! {
    pub(crate) struct ProcessTextFlags: u8 {
        const NoCombining = 1;
        const StripClozes = 1 << 1;
    }
}

fn add_process_text_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "process_text",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let mut text = Cow::from(ctx.get_raw(0).as_str()?);
            let opt = ProcessTextFlags::from_bits_truncate(ctx.get_raw(1).as_i64()? as u8);
            if opt.contains(ProcessTextFlags::StripClozes) {
                text = text.map_cow(strip_clozes);
            }
            if opt.contains(ProcessTextFlags::NoCombining) {
                text = text.map_cow(without_combining);
            }
            Ok(text.get_owned())
        },
    )
}

fn add_fnvhash_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function("fnvhash", -1, FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let mut hasher = FnvHasher::default();
        for idx in 0..ctx.len() {
            hasher.write_i64(ctx.get(idx)?);
        }
        Ok(hasher.finish() as i64)
    })
}

/// Adds sql function regexp(regex, string) -> is_match
/// Taken from the rusqlite docs
type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
fn add_regexp_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");

            let re: Arc<Regex> = ctx
                .get_or_create_aux(0, |vr| -> std::result::Result<_, BoxError> {
                    Ok(Regex::new(vr.as_str()?)?)
                })?;

            let is_match = {
                let text = ctx
                    .get_raw(1)
                    .as_str()
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

                re.is_match(text)
            };

            Ok(is_match)
        },
    )
}

/// Adds sql function `regexp_fields(regex, note_flds, indices...) -> is_match`.
/// If no indices are provided, all fields are matched against.
fn add_regexp_fields_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "regexp_fields",
        -1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert!(ctx.len() > 1, "not enough arguments");

            let re: Arc<Regex> = ctx
                .get_or_create_aux(0, |vr| -> std::result::Result<_, BoxError> {
                    Ok(Regex::new(vr.as_str()?)?)
                })?;
            let fields = ctx.get_raw(1).as_str()?.split('\x1f');
            let indices: HashSet<usize> = (2..ctx.len())
                .map(|i| ctx.get(i))
                .collect::<rusqlite::Result<_>>()?;

            Ok(fields.enumerate().any(|(idx, field)| {
                (indices.is_empty() || indices.contains(&idx)) && re.is_match(field)
            }))
        },
    )
}

/// Adds sql function `regexp_tags(regex, tags) -> is_match`.
fn add_regexp_tags_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "regexp_tags",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");

            let re: Arc<Regex> = ctx
                .get_or_create_aux(0, |vr| -> std::result::Result<_, BoxError> {
                    Ok(Regex::new(vr.as_str()?)?)
                })?;
            let mut tags = ctx.get_raw(1).as_str()?.split(' ');

            Ok(tags.any(|tag| re.is_match(tag)))
        },
    )
}

/// eg. extract_original_position(c.data) -> number | null
/// Parse original card position from c.data (this is only populated after card
/// has been reviewed)
fn add_extract_original_position_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "extract_original_position",
        1,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");

            let Ok(card_data) = ctx.get_raw(0).as_str() else {
                return Ok(None);
            };

            match &CardData::from_str(card_data).original_position {
                Some(position) => Ok(Some(*position as i64)),
                None => Ok(None),
            }
        },
    )
}

/// eg. extract_custom_data(card.data, 'r') -> string | null
fn add_extract_custom_data_function(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "extract_custom_data",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");

            let Ok(card_data) = ctx.get_raw(0).as_str() else {
                return Ok(None);
            };
            if card_data.is_empty() {
                return Ok(None);
            }
            let Ok(key) = ctx.get_raw(1).as_str() else {
                return Ok(None);
            };
            let custom_data = &CardData::from_str(card_data).custom_data;
            let Ok(value) = serde_json::from_str::<Value>(custom_data) else {
                return Ok(None);
            };
            let v = value.get(key).map(|v| match v {
                Value::String(s) => s.to_owned(),
                _ => v.to_string(),
            });
            Ok(v)
        },
    )
}

/// eg. extract_fsrs_variable(card.data, 's' | 'd' | 'dr') -> float | null
fn add_extract_fsrs_variable(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "extract_fsrs_variable",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");

            let Ok(card_data) = ctx.get_raw(0).as_str() else {
                return Ok(None);
            };
            if card_data.is_empty() {
                return Ok(None);
            }
            let Ok(key) = ctx.get_raw(1).as_str() else {
                return Ok(None);
            };
            let card_data = &CardData::from_str(card_data);
            Ok(match key {
                "s" => card_data.fsrs_stability,
                "d" => card_data.fsrs_difficulty,
                "dr" => card_data.fsrs_desired_retention,
                _ => panic!("invalid key: {key}"),
            })
        },
    )
}

/// eg. extract_fsrs_retrievability(card.data, card.due, card.ivl,
/// timing.days_elapsed, timing.next_day_at, timing.now) -> float | null
fn add_extract_fsrs_retrievability(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "extract_fsrs_retrievability",
        6,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 6, "called with unexpected number of arguments");
            let Ok(card_data) = ctx.get_raw(0).as_str() else {
                return Ok(None);
            };
            if card_data.is_empty() {
                return Ok(None);
            }
            let card_data = &CardData::from_str(card_data);
            let Ok(due) = ctx.get_raw(1).as_i64() else {
                return Ok(None);
            };
            let Ok(now) = ctx.get_raw(5).as_i64() else {
                return Ok(None);
            };
            let seconds_elapsed = if let Some(last_review_time) = card_data.last_review_time {
                // This and any following
                // (x as u32).saturating_sub(y as u32)
                // must not be changed to
                // x.saturating_sub(y) as u32
                // as x and y are i64's and saturating_sub will therfore allow negative numbers
                // before converting to u32 in the latter example.
                (now as u32).saturating_sub(last_review_time.0 as u32)
            } else if due > 365_000 {
                // (re)learning card in seconds
                let Ok(ivl) = ctx.get_raw(2).as_i64() else {
                    return Ok(None);
                };
                let last_review_time = (due as u32).saturating_sub(ivl as u32);
                (now as u32).saturating_sub(last_review_time)
            } else {
                let Ok(ivl) = ctx.get_raw(2).as_i64() else {
                    return Ok(None);
                };
                // timing.days_elapsed
                let Ok(today) = ctx.get_raw(3).as_i64() else {
                    return Ok(None);
                };
                let review_day = (due as u32).saturating_sub(ivl as u32);
                (today as u32).saturating_sub(review_day) * 86_400
            };
            let decay = card_data.decay.unwrap_or(FSRS5_DEFAULT_DECAY);
            let retrievability = card_data.memory_state().map(|state| {
                fsrs::current_retrievability(state.into(), seconds_elapsed as f32 / 86_400.0, decay)
            });
            Ok(retrievability)
        },
    )
}

/// eg. extract_fsrs_relative_retrievability(card.data, card.due,
/// card.ivl, timing.days_elapsed, timing.next_day_at, timing.now) -> float |
/// null. The higher the number, the higher the card's retrievability relative
/// to the configured desired retention.
fn add_extract_fsrs_relative_retrievability(db: &Connection) -> rusqlite::Result<()> {
    db.create_scalar_function(
        "extract_fsrs_relative_retrievability",
        6,
        FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 6, "called with unexpected number of arguments");

            let Ok(due) = ctx.get_raw(1).as_i64() else {
                return Ok(None);
            };
            let Ok(interval) = ctx.get_raw(2).as_i64() else {
                return Ok(None);
            };
            /*
            // Unused
            let Ok(next_day_at) = ctx.get_raw(4).as_i64() else {
                return Ok(None);
            };
            */
            let Ok(now) = ctx.get_raw(5).as_i64() else {
                return Ok(None);
            };
            let secs_elapsed = if due > 365_000 {
                // (re)learning card with due in seconds

                // Don't change this to now.subtracting_sub(due) as u32
                // for the same reasons listed in the comment
                // in add_extract_fsrs_retrievability
                (now as u32).saturating_sub(due as u32)
            } else {
                // timing.days_elapsed
                let Ok(today) = ctx.get_raw(3).as_i64() else {
                    return Ok(None);
                };
                let review_day = due.saturating_sub(interval);
                (today as u32).saturating_sub(review_day as u32) * 86_400
            };
            if let Ok(card_data) = ctx.get_raw(0).as_str() {
                if !card_data.is_empty() {
                    let card_data = &CardData::from_str(card_data);
                    if let (Some(state), Some(mut desired_retrievability)) =
                        (card_data.memory_state(), card_data.fsrs_desired_retention)
                    {
                        // avoid div by zero
                        desired_retrievability = desired_retrievability.max(0.0001);
                        let decay = card_data.decay.unwrap_or(FSRS5_DEFAULT_DECAY);

                        let seconds_elapsed =
                            if let Some(last_review_time) = card_data.last_review_time {
                                // Don't change this to now.subtracting_sub(due) as u32
                                // for the same reasons listed in the comment
                                // in add_extract_fsrs_retrievability
                                (now as u32).saturating_sub(last_review_time.0 as u32)
                            } else {
                                secs_elapsed
                            };

                        let current_retrievability = fsrs::current_retrievability(
                            state.into(),
                            seconds_elapsed as f32 / 86_400.0,
                            decay,
                        )
                        .max(0.0001);

                        return Ok(Some(
                            -(current_retrievability.powf(-1.0 / decay) - 1.)
                                / (desired_retrievability.powf(-1.0 / decay) - 1.),
                        ));
                    }
                }
            }
            let days_elapsed = secs_elapsed / 86_400;
            // FSRS data missing; fall back on how overdue it is relative to its interval
            Ok(Some(
                -((days_elapsed as f32) + 0.001) / (interval as f32).max(1.0),
            ))
        },
    )
}

/// Fetch schema version from database.
/// Return (must_create, version)
fn schema_version(db: &Connection) -> Result<(bool, u8)> {
    if !db
        .prepare("select null from sqlite_master where type = 'table' and name = 'col'")?
        .exists([])?
    {
        return Ok((true, SCHEMA_STARTING_VERSION));
    }

    Ok((
        false,
        db.query_row("select ver from col", [], |r| r.get(0))?,
    ))
}

fn trace(event: TraceEvent) {
    if let TraceEvent::Stmt(_, sql) = event {
        println!("sql: {}", sql.trim().replace('\n', " "));
    }
}

impl SqliteStorage {
    pub(crate) fn open_or_create(path: &Path, tr: &I18n, check_integrity: bool) -> Result<Self> {
        let db = open_or_create_collection_db(path)?;
        let (create, ver) = schema_version(&db)?;

        let err = match ver {
            v if v < SCHEMA_MIN_VERSION => Some(DbErrorKind::FileTooOld),
            v if v > SCHEMA_MAX_VERSION => Some(DbErrorKind::FileTooNew),
            12 | 13 => {
                // as schema definition changed, user must perform clean
                // shutdown to return to schema 11 prior to running this version
                Some(DbErrorKind::FileTooNew)
            }
            _ => None,
        };
        if let Some(kind) = err {
            return Err(AnkiError::db_error("", kind));
        }

        if check_integrity {
            let s =
                db.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))?;
            require!(s == "ok", "corrupt: {s}");
        }

        let upgrade = ver != SCHEMA_MAX_VERSION;
        if create || upgrade {
            db.execute("begin exclusive", [])?;
        }

        if create {
            db.execute_batch(include_str!("schema11.sql"))?;
            // start at schema 11, then upgrade below
            let crt = TimestampSecs(v1_creation_date());
            let offset = Some(local_minutes_west_for_stamp(crt)?);
            db.execute(
                "update col set crt=?, ver=?, conf=?",
                params![
                    crt,
                    SCHEMA_STARTING_VERSION,
                    &schema11_config_as_string(offset)
                ],
            )?;
        }

        let storage = Self { db };

        if create || upgrade {
            storage.upgrade_to_latest_schema(ver)?;
        }

        if create {
            storage.add_default_deck(tr)?;
            storage.add_stock_notetypes(tr)?;
        }

        if create || upgrade {
            storage.commit_trx()?;
        }

        // Like the old day rollover, this leaves mod alone.
        storage.db.execute(
            &format!("UPDATE cards SET queue = {QUEUE_ONCE_OPENED_SQL} WHERE queue IN (-2, -3)"),
            [],
        )?;

        Ok(storage)
    }

    // Standard transaction start/stop
    //////////////////////////////////////

    pub(crate) fn commit_trx(&self) -> Result<()> {
        if !self.db.is_autocommit() {
            self.db.prepare_cached("commit")?.execute([])?;
        }
        Ok(())
    }

    pub(crate) fn rollback_trx(&self) -> Result<()> {
        if !self.db.is_autocommit() {
            self.db.execute("rollback", [])?;
        }
        Ok(())
    }

    // Savepoints
    //////////////////////////////////////////
    //
    // This is necessary at the moment because Anki's current architecture uses
    // long-running transactions as an undo mechanism. Once a proper undo
    // mechanism has been added to all existing functionality, we could
    // transition these to standard commits.

    pub(crate) fn begin_rust_trx(&self) -> Result<()> {
        self.db.prepare_cached("savepoint rust")?.execute([])?;
        Ok(())
    }

    pub(crate) fn commit_rust_trx(&self) -> Result<()> {
        self.db.prepare_cached("release rust")?.execute([])?;
        Ok(())
    }

    pub(crate) fn rollback_rust_trx(&self) -> Result<()> {
        self.db.prepare_cached("rollback to rust")?.execute([])?;
        Ok(())
    }

    //////////////////////////////////////////

    /// true if corrupt/can't access
    pub(crate) fn quick_check_corrupt(&self) -> bool {
        match self.db.pragma_query_value(None, "quick_check", |row| {
            row.get(0).map(|v: String| v != "ok")
        }) {
            Ok(corrupt) => corrupt,
            Err(e) => {
                println!("error: {e:?}");
                true
            }
        }
    }

    pub(crate) fn optimize(&self) -> Result<()> {
        self.db.execute_batch("vacuum; reindex; analyze")?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn db_scalar<T: rusqlite::types::FromSql>(&self, sql: &str) -> Result<T> {
        self.db.query_row(sql, [], |r| r.get(0)).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SqlSortOrder {
    Ascending,
    Descending,
}

impl Display for SqlSortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SqlSortOrder::Ascending => "asc",
                SqlSortOrder::Descending => "desc",
            }
        )
    }
}

/// The schema version of a collection nothing has open, without changing it.
pub fn stored_schema_version(path: &Path) -> Result<u8> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(db.query_row("select ver from col", [], |row| row.get(0))?)
}

/// Whether opening a collection nothing has open would upgrade its schema,
/// which can't be undone, or whether that can't be told.
pub fn schema_upgrade_needed(path: &Path) -> bool {
    stored_schema_version(path).map_or(true, |ver| ver < SCHEMA_MAX_VERSION)
}

/// Copy a database with SQLite's online backup, so a live one stays consistent:
/// a plain file copy could tear a write, or miss what's in the WAL. Replaces
/// nothing, as the destination must be a new file. Doesn't need the collation
/// or functions of a collection.
pub fn copy_database(source: &Connection, destination: &Path) -> Result<()> {
    let mut output = Connection::open(destination)?;
    {
        let backup = rusqlite::backup::Backup::new(source, &mut output)?;
        // in one step: the caller holds the collection, so it can't change, and
        // this way nothing can restart the copy
        if !matches!(backup.step(-1)?, rusqlite::backup::StepResult::Done) {
            return Err(AnkiError::db_error(
                "database copy stopped early",
                DbErrorKind::Other,
            ));
        }
    }
    // the header came from the source, and a copy has no use for a WAL file
    output.pragma_update(None, "journal_mode", "delete")?;
    Ok(())
}

/// Check a copy of a collection, opened without changing it. This is everything
/// but the indexes' contents, which is what takes the time.
pub fn verify_copied_collection(path: &Path) -> Result<()> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.create_collation("unicase", unicase_compare)?;
    let result: String = db.pragma_query_value(None, "quick_check", |row| row.get(0))?;
    if result != "ok" {
        return Err(AnkiError::db_error(result, DbErrorKind::Corrupt));
    }
    let _: i64 = db.query_row("select count() from notes", [], |row| row.get(0))?;
    Ok(())
}

impl SqliteStorage {
    /// Something that changes when reviews or edits do, for telling if a backup
    /// is needed. Reads little, as it's asked for regularly.
    pub fn change_stamp(&self) -> Result<(i64, i64, i64)> {
        Ok(self.db.query_row(
            "select (select mod from col), (select count() from revlog), \
             (select coalesce(max(id), 0) from revlog)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?)
    }
}

#[cfg(test)]
mod test {
    use anki_io::new_tempfile;

    use super::*;
    use crate::collection::CollectionBuilder;
    use crate::error::DbError;
    use crate::scheduler::answering::test::v3_test_collection;
    use crate::search::SortMode;
    use crate::storage::card::ReviewOrderSubclause;
    use crate::tests::NoteAdder;

    fn test_tr() -> I18n {
        I18n::template_only()
    }

    #[test]
    fn opening_returns_cards_in_old_queues_to_normal() {
        let tempfile = new_tempfile().unwrap();
        // (queue, type, due), and the queue expected after opening
        let cases = [
            ((-2, 0, 7), 0),
            ((-3, 0, 7), 0),
            ((-2, 1, 1_700_000_000), 1),
            ((-3, 1, 7), 3),
            ((-2, 2, 7), 2),
            ((-3, 2, 7), 2),
            ((-2, 3, 1_700_000_000), 1),
            ((-3, 3, 7), 3),
            // these aren't touched
            ((-1, 2, 7), -1),
            ((2, 2, 7), 2),
        ];
        let mut cids = vec![];
        {
            let mut col = CollectionBuilder::default()
                .set_collection_path(tempfile.path())
                .build()
                .unwrap();
            for ((queue, ctype, due), _) in cases {
                let nt = col.get_notetype_by_name("Basic").unwrap().unwrap();
                let mut note = nt.new_note();
                note.set_field(0, "front").unwrap();
                col.add_note(&mut note, DeckId(1)).unwrap();
                let cid: CardId = col
                    .storage
                    .db
                    .query_row("select id from cards where nid = ?", [note.id], |row| {
                        row.get(0)
                    })
                    .unwrap();
                col.storage
                    .db
                    .execute(
                        "update cards set queue = ?, type = ?, due = ?, mod = 5 \
                         where id = ?",
                        params![queue, ctype, due, cid],
                    )
                    .unwrap();
                cids.push(cid);
            }
            col.close().unwrap();
        }

        let mut col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()
            .unwrap();
        for (((_, ctype, due), queue), cid) in cases.into_iter().zip(cids) {
            let raw: (i8, i8, i32, i64) = col
                .storage
                .db
                .query_row(
                    "select queue, type, due, mod from cards where id = ?",
                    [cid],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
            assert_eq!(raw, (queue, ctype, due, 5));
            // and it's what the backend would do itself
            let mut card = col.storage.get_card(cid).unwrap().unwrap();
            if queue != -1 {
                card.restore_queue_from_type();
                assert_eq!(card.queue as i8, queue);
            }
        }
        assert_eq!(
            col.search_cards("is:suspended", SortMode::NoOrder)
                .unwrap()
                .len(),
            1
        );
    }

    /// Builds a fresh on-disk collection, stamps the given schema version into
    /// the `col` table, closes it without downgrading, then reopens it through
    /// the migration gate and returns the resulting error kind.
    fn open_error_kind_for_schema_version(ver: u8) -> DbErrorKind {
        let tempfile = new_tempfile().unwrap();
        {
            let col = CollectionBuilder::default()
                .set_collection_path(tempfile.path())
                .build()
                .unwrap();
            col.storage
                .db
                .execute("update col set ver = ?", params![ver])
                .unwrap();
            col.close().unwrap();
        }
        let err = SqliteStorage::open_or_create(tempfile.path(), &test_tr(), false).unwrap_err();
        match err {
            AnkiError::DbError {
                source: DbError { kind, .. },
            } => kind,
            other => panic!("expected DbError, got {other:?}"),
        }
    }

    /// Reads the persisted schema version with a raw connection, so no upgrade
    /// is triggered.
    fn stored_version(path: &Path) -> u8 {
        stored_schema_version(path).unwrap()
    }

    #[test]
    fn open_rejects_schema_older_than_minimum() {
        assert_eq!(
            open_error_kind_for_schema_version(SCHEMA_MIN_VERSION - 1),
            DbErrorKind::FileTooOld
        );
    }

    #[test]
    fn open_rejects_schema_newer_than_maximum() {
        assert_eq!(
            open_error_kind_for_schema_version(SCHEMA_MAX_VERSION + 1),
            DbErrorKind::FileTooNew
        );
    }

    #[test]
    fn open_rejects_intermediate_schema_versions_12_and_13() {
        // versions 12 and 13 require a clean shutdown back to 11 first
        assert_eq!(
            open_error_kind_for_schema_version(12),
            DbErrorKind::FileTooNew
        );
        assert_eq!(
            open_error_kind_for_schema_version(13),
            DbErrorKind::FileTooNew
        );
    }

    #[test]
    fn rejected_schema_leaves_version_and_collection_data_unchanged() {
        for ver in [SCHEMA_MIN_VERSION - 1, 12, 13, SCHEMA_MAX_VERSION + 1] {
            let tempfile = new_tempfile().unwrap();
            let mut col = CollectionBuilder::default()
                .set_collection_path(tempfile.path())
                .build()
                .unwrap();
            let note = NoteAdder::basic(&mut col)
                .fields(&["Preserved question", "Preserved answer"])
                .add(&mut col);
            let original_note = col.storage.get_note(note.id).unwrap().unwrap();
            let cards = col.storage.get_all_cards();
            col.storage
                .db
                .execute("update col set ver = ?", params![ver])
                .unwrap();
            let collection_change = TimestampMillis(1_600_000_000_000);
            col.storage.set_modified_time(collection_change).unwrap();
            col.close().unwrap();

            SqliteStorage::open_or_create(tempfile.path(), &test_tr(), false).unwrap_err();

            assert_eq!(stored_version(tempfile.path()), ver, "schema {ver}");
            // A rejected open must be side-effect free, so the modification time
            // is left untouched (read before restoring the version below).
            let mod_after: i64 = Connection::open(tempfile.path())
                .unwrap()
                .query_row("select mod from col", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mod_after, collection_change.0, "schema {ver}");
            // Only the version marker was changed in setup; restore it to verify
            // that rejection left the collection readable and its data intact.
            Connection::open(tempfile.path())
                .unwrap()
                .execute("update col set ver = ?", params![SCHEMA_MAX_VERSION])
                .unwrap();
            let col = CollectionBuilder::default()
                .set_collection_path(tempfile.path())
                .build()
                .unwrap();
            assert_eq!(
                col.storage.get_note(note.id).unwrap(),
                Some(original_note),
                "schema {ver}"
            );
            assert_eq!(col.storage.get_all_cards(), cards, "schema {ver}");
        }
    }

    #[test]
    fn fresh_collection_is_created_at_latest_schema_version() {
        let tempfile = new_tempfile().unwrap();
        let col = CollectionBuilder::default()
            .set_collection_path(tempfile.path())
            .build()
            .unwrap();

        let ver: u8 = col.storage.db_scalar("select ver from col").unwrap();

        assert_eq!(ver, SCHEMA_MAX_VERSION);
    }

    #[test]
    fn missing_memory_state_falls_back_on_relative_overdueness() -> Result<()> {
        let (mut col, _cids) = v3_test_collection(1)?;
        col.answer_easy();

        let timing = col.timing_today()?;
        let sql_func = ReviewOrderSubclause::RelativeOverdueness { timing }
            .to_string()
            .replace(" asc", "");
        let sql = format!("select {sql_func} from cards");

        // value from fsrs
        let mut pos: Option<f64>;
        pos = col.storage.db_scalar(&sql).unwrap();
        assert_eq!(pos, Some(0.0));
        // erasing the memory state should not result in None output
        col.storage.db.execute("update cards set data=''", [])?;
        pos = col.storage.db_scalar(&sql).unwrap();
        assert!(pos.is_some());
        // but it won't match the fsrs value
        assert!(pos.unwrap() < -0.0);
        Ok(())
    }
}
