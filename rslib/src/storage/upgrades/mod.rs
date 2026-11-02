// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/// The minimum schema version we can open.
pub(super) const SCHEMA_MIN_VERSION: u8 = 11;
/// The version new files are initially created with.
pub(super) const SCHEMA_STARTING_VERSION: u8 = 11;
/// The maximum schema version we can open.
pub(super) const SCHEMA_MAX_VERSION: u8 = 19;

use super::SqliteStorage;
use crate::error::Result;

impl SqliteStorage {
    pub(super) fn upgrade_to_latest_schema(&self, ver: u8) -> Result<()> {
        if ver < 14 {
            self.db
                .execute_batch(include_str!("schema14_upgrade.sql"))?;
            self.drop_usn_columns()?;
            self.upgrade_deck_conf_to_schema14()?;
            self.upgrade_tags_to_schema14()?;
            self.upgrade_config_to_schema14()?;
        }
        if ver < 15 {
            self.db
                .execute_batch(include_str!("schema15_upgrade.sql"))?;
            self.drop_usn_columns()?;
            self.upgrade_notetypes_to_schema15()?;
            self.upgrade_decks_to_schema15()?;
            self.upgrade_deck_conf_to_schema15()?;
        }
        if ver < 16 {
            self.upgrade_deck_conf_to_schema16()?;
            self.db.execute_batch("update col set ver = 16")?;
        }
        if ver < 17 {
            self.upgrade_tags_to_schema17()?;
            self.db.execute_batch("update col set ver = 17")?;
        }
        if ver < 18 {
            self.db
                .execute_batch(include_str!("schema18_upgrade.sql"))?;
        }
        if ver < 19 {
            self.drop_usn_columns()?;
            self.db
                .execute_batch(include_str!("schema19_upgrade.sql"))?;
        }

        Ok(())
    }

    /// Schema 19 has no usn columns, and the steps above write through the same
    /// statements as everything else, so the columns have to be gone before a
    /// step that writes to a table, including one that a step before it has
    /// just created. The caller's transaction makes all of this one change.
    pub(in crate::storage) fn drop_usn_columns(&self) -> Result<()> {
        let tables = self.tables_with_usn_column()?;
        for table in tables {
            // a column in an index can't be dropped
            for index in self.indexes_on_usn(&table)? {
                self.db.execute_batch(&format!("DROP INDEX {index}"))?;
            }
            self.db
                .execute_batch(&format!("ALTER TABLE {table} DROP COLUMN usn"))?;
        }
        Ok(())
    }

    /// Not the graves table, which goes as a whole, or the col table, whose
    /// sync columns go together with it, as their value doesn't matter until
    /// then.
    fn tables_with_usn_column(&self) -> Result<Vec<String>> {
        self.db
            .prepare(
                "select m.name from sqlite_master m, pragma_table_info(m.name) c
                 where m.type = 'table' and c.name = 'usn'
                   and m.name not in ('graves', 'col')",
            )?
            .query_and_then([], |row| Ok(row.get(0)?))?
            .collect()
    }

    fn indexes_on_usn(&self, table: &str) -> Result<Vec<String>> {
        self.db
            .prepare(
                "select m.name from sqlite_master m, pragma_index_info(m.name) c
                 where m.type = 'index' and m.tbl_name = ? and c.name = 'usn'",
            )?
            .query_and_then([table], |row| Ok(row.get(0)?))?
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod test {
    use std::collections::BTreeMap;
    use std::path::Path;

    use anki_io::new_tempfile;
    use rusqlite::types::Value;
    use rusqlite::Connection;

    use crate::collection::CollectionBuilder;
    use crate::prelude::*;
    use crate::tests::NoteAdder;

    const MEDIACHECK: &[u8] = include_bytes!("../../../tests/support/mediacheck.anki2");

    /// Give a closed collection what schema 18 had and this doesn't, with an
    /// arbitrary value in it.
    pub(crate) fn turn_into_schema_18(db: &Connection) {
        for table in [
            "notes",
            "cards",
            "revlog",
            "deck_config",
            "config",
            "templates",
            "notetypes",
            "decks",
            "tags",
        ] {
            db.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN usn integer NOT NULL DEFAULT 42"
            ))
            .unwrap();
        }
        db.execute_batch(
            "CREATE INDEX ix_notes_usn ON notes (usn);
             CREATE INDEX ix_cards_usn ON cards (usn);
             CREATE INDEX ix_revlog_usn ON revlog (usn);
             CREATE INDEX idx_templates_usn ON templates (usn);
             CREATE INDEX idx_notetypes_usn ON notetypes (usn);
             CREATE TABLE graves (
               oid integer NOT NULL,
               type integer NOT NULL,
               usn integer NOT NULL,
               PRIMARY KEY (oid, type)
             ) WITHOUT ROWID;
             CREATE INDEX idx_graves_pending ON graves (usn);
             INSERT INTO graves VALUES (123, 0, -1);
             ALTER TABLE col ADD COLUMN scm integer NOT NULL DEFAULT 5;
             ALTER TABLE col ADD COLUMN dty integer NOT NULL DEFAULT 6;
             ALTER TABLE col ADD COLUMN usn integer NOT NULL DEFAULT 7;
             ALTER TABLE col ADD COLUMN ls integer NOT NULL DEFAULT 8;
             UPDATE col SET ver = 18;",
        )
        .unwrap();
    }

    /// Every table but SQLite's own, with every column but the sync ones.
    fn contents(path: &Path) -> BTreeMap<String, Vec<Vec<Value>>> {
        let db = Connection::open(path).unwrap();
        db.create_collation("unicase", |a, b| a.cmp(b)).unwrap();
        let mut tables: Vec<String> = db
            .prepare(
                "select name from sqlite_master where type = 'table'
                 and name not like 'sqlite_%' and name != 'graves'",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|name| name.unwrap())
            .collect();
        tables.sort();
        tables
            .into_iter()
            .map(|table| {
                let mut columns: Vec<String> = db
                    .prepare("select name from pragma_table_info(?)")
                    .unwrap()
                    .query_map([&table], |row| row.get(0))
                    .unwrap()
                    .map(|name| name.unwrap())
                    .filter(|name: &String| {
                        !matches!(name.as_str(), "usn" | "scm" | "dty" | "ls" | "ver")
                    })
                    .collect();
                columns.sort();
                let sql = format!("select {} from {table} order by 1, 2", columns.join(","));
                let mut statement = db.prepare(&sql).unwrap();
                let width = statement.column_count();
                let rows = statement
                    .query_map([], |row| {
                        (0..width)
                            .map(|i| row.get(i))
                            .collect::<rusqlite::Result<_>>()
                    })
                    .unwrap()
                    .map(|row| row.unwrap())
                    .collect();
                (table, rows)
            })
            .collect()
    }

    fn sync_leftovers(path: &Path) -> Vec<String> {
        let db = Connection::open(path).unwrap();
        let mut statement = db
            .prepare(
                "select m.name || '.' || c.name from sqlite_master m, pragma_table_info(m.name) c
             where m.type = 'table' and (c.name = 'usn' or
               (m.name = 'col' and c.name in ('scm', 'dty', 'ls')))
             union all select name from sqlite_master
             where name = 'graves' or name like '%usn%' or name like '%graves%'",
            )
            .unwrap();
        let names = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|name| name.unwrap())
            .collect();
        names
    }

    fn version(path: &Path) -> u8 {
        crate::storage::stored_schema_version(path).unwrap()
    }

    fn open(path: &Path) -> Collection {
        CollectionBuilder::new(path).build().unwrap()
    }

    /// A collection with something in each table, at schema 18.
    fn schema_18_file() -> tempfile::NamedTempFile {
        let file = new_tempfile().unwrap();
        {
            let mut col = open(file.path());
            NoteAdder::basic(&mut col)
                .fields(&["question", "answer"])
                .add(&mut col);
            col.answer_easy();
            col.set_config("migrationTest", &vec![1, 2, 3]).unwrap();
            col.storage
                .db
                .execute(
                    "insert into tags (tag, collapsed) values ('a::b', true)",
                    [],
                )
                .unwrap();
            col.close().unwrap();
        }
        turn_into_schema_18(&Connection::open(file.path()).unwrap());
        file
    }

    #[test]
    fn schema_18_loses_only_what_tracked_sync() {
        let file = schema_18_file();
        let before = contents(file.path());
        assert!(!sync_leftovers(file.path()).is_empty());
        assert!(before["revlog"].len() == 1 && before["cards"].len() == 1);

        open(file.path()).close().unwrap();

        assert_eq!(version(file.path()), 19);
        assert_eq!(sync_leftovers(file.path()), Vec::<String>::new());
        assert_eq!(contents(file.path()), before);
    }

    #[test]
    fn opening_again_changes_nothing() {
        let file = schema_18_file();
        open(file.path()).close().unwrap();
        let once = contents(file.path());
        let schema_once: Vec<String> = Connection::open(file.path())
            .unwrap()
            .prepare("select sql from sqlite_master order by name")
            .unwrap()
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .unwrap()
            .map(|sql| sql.unwrap().unwrap_or_default())
            .collect();

        open(file.path()).close().unwrap();

        assert_eq!(contents(file.path()), once);
        let schema_twice: Vec<String> = Connection::open(file.path())
            .unwrap()
            .prepare("select sql from sqlite_master order by name")
            .unwrap()
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .unwrap()
            .map(|sql| sql.unwrap().unwrap_or_default())
            .collect();
        assert_eq!(schema_twice, schema_once);
    }

    /// Something that stops one of the last statements leaves it as it was.
    #[test]
    fn a_failed_upgrade_changes_nothing() {
        let file = schema_18_file();
        let db = Connection::open(file.path()).unwrap();
        // a column a view uses can't be dropped, and this table comes late
        db.execute_batch("CREATE VIEW blocker AS SELECT ls FROM col")
            .unwrap();
        drop(db);
        let before = contents(file.path());

        CollectionBuilder::new(file.path()).build().unwrap_err();

        assert_eq!(version(file.path()), 18);
        assert_eq!(contents(file.path()), before);
        let leftovers = sync_leftovers(file.path());
        assert!(leftovers.contains(&"cards.usn".to_string()));
        assert!(leftovers.contains(&"graves".to_string()));
        assert!(leftovers.contains(&"col.ls".to_string()));
    }

    #[test]
    fn schema_11_upgrades_with_nothing_of_sync_left() {
        let file = new_tempfile().unwrap();
        std::fs::write(file.path(), MEDIACHECK).unwrap();
        let (notes, cards, revlog): (i64, i64, i64) = {
            let db = Connection::open(file.path()).unwrap();
            assert_eq!(
                db.query_row("select ver from col", [], |r| r.get::<_, u8>(0))
                    .unwrap(),
                11
            );
            db.execute(
                r#"update col set tags = '{"one::two": 5, "three": -1}'"#,
                [],
            )
            .unwrap();
            db.query_row(
                "select (select count() from notes), (select count() from cards),
                        (select count() from revlog)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };

        let col = open(file.path());

        assert_eq!(
            col.storage.db_scalar::<u8>("select ver from col").unwrap(),
            19
        );
        assert_eq!(
            col.storage
                .db_scalar::<i64>(
                    "select (select count() from notes) * 10000
                          + (select count() from cards) * 100
                          + (select count() from revlog)"
                )
                .unwrap(),
            notes * 10000 + cards * 100 + revlog
        );
        let mut tags: Vec<String> = col
            .storage
            .all_tags()
            .unwrap()
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        tags.sort();
        assert_eq!(tags, ["one::two", "three"]);
        // the usn of old JSON isn't kept, in the columns or with unknown keys
        let notetypes = col.storage.get_all_notetype_names().unwrap();
        assert!(!notetypes.is_empty());
        for (id, _) in notetypes {
            let notetype = col.storage.get_notetype(id).unwrap().unwrap();
            assert!(!String::from_utf8_lossy(&notetype.config.other).contains("usn"));
        }
        for deck in col.storage.get_all_decks().unwrap() {
            assert!(!String::from_utf8_lossy(&deck.common.other).contains("usn"));
        }
        for config in col.storage.all_deck_config_unchecked().unwrap() {
            assert!(!String::from_utf8_lossy(&config.inner.other).contains("usn"));
        }
        col.close().unwrap();
        assert_eq!(sync_leftovers(file.path()), Vec::<String>::new());
    }

    /// A new collection starts at schema 11, so this runs every step.
    #[test]
    fn a_new_collection_has_nothing_of_sync() {
        let file = new_tempfile().unwrap();
        let mut col = open(file.path());
        assert_eq!(
            col.storage.db_scalar::<u8>("select ver from col").unwrap(),
            19
        );
        let note = NoteAdder::basic(&mut col).add(&mut col);
        assert!(col.storage.get_note(note.id).unwrap().is_some());
        col.close().unwrap();
        assert_eq!(sync_leftovers(file.path()), Vec::<String>::new());
    }
}
