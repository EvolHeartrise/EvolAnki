// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::path::Path;

use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Row;

use crate::error;
use crate::media::Sha1Hash;
use crate::prelude::*;

pub mod changetracker;

pub struct Checksums(HashMap<String, Sha1Hash>);

impl Checksums {
    // case-fold filenames when checking files to be imported
    // to account for case-insensitive filesystems
    pub fn get(&self, key: impl AsRef<str>) -> Option<&Sha1Hash> {
        self.0.get(key.as_ref().to_lowercase().as_str())
    }

    pub fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.get(key).is_some()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MediaEntry {
    pub fname: String,
    /// None only if what's stored can't be read.
    pub sha1: Option<Sha1Hash>,
    pub mtime: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MediaDatabaseMetadata {
    pub folder_mtime: i64,
}

pub struct MediaDatabase {
    db: Connection,
}

impl MediaDatabase {
    pub(crate) fn new(db_path: &Path) -> error::Result<MediaDatabase> {
        Ok(MediaDatabase {
            db: open_or_create(db_path)?,
        })
    }

    /// Execute the provided closure in a transaction, rolling back if
    /// an error is returned.
    pub(crate) fn transact<F, R>(&self, func: F) -> error::Result<R>
    where
        F: FnOnce(&MediaDatabase) -> error::Result<R>,
    {
        self.begin()?;

        let mut res = func(self);

        if res.is_ok() {
            if let Err(e) = self.commit() {
                res = Err(e);
            }
        }

        if res.is_err() {
            self.rollback()?;
        }

        res
    }

    fn begin(&self) -> error::Result<()> {
        self.db.execute_batch("begin immediate").map_err(Into::into)
    }

    fn commit(&self) -> error::Result<()> {
        self.db.execute_batch("commit").map_err(Into::into)
    }

    fn rollback(&self) -> error::Result<()> {
        self.db.execute_batch("rollback").map_err(Into::into)
    }

    pub(crate) fn get_entry(&self, fname: &str) -> error::Result<Option<MediaEntry>> {
        self.db
            .prepare_cached(
                "
select fname, csum, mtime from media where fname=?",
            )?
            .query_row(params![fname], row_to_entry)
            .optional()
            .map_err(Into::into)
    }

    pub(crate) fn set_entry(&self, entry: &MediaEntry) -> error::Result<()> {
        let sha1_str = entry.sha1.map(hex::encode);
        self.db
            .prepare_cached(
                "
insert or replace into media (fname, csum, mtime)
values (?, ?, ?)",
            )?
            .execute(params![entry.fname, sha1_str, entry.mtime])?;

        Ok(())
    }

    pub(crate) fn remove_entry(&self, fname: &str) -> error::Result<()> {
        self.db
            .prepare_cached("delete from media where fname = ?")?
            .execute([fname])?;
        Ok(())
    }

    pub(crate) fn get_meta(&self) -> error::Result<MediaDatabaseMetadata> {
        let mut stmt = self.db.prepare("select dirMod from meta")?;

        stmt.query_row([], |row| {
            Ok(MediaDatabaseMetadata {
                folder_mtime: row.get(0)?,
            })
        })
        .map_err(Into::into)
    }

    pub(crate) fn set_meta(&self, meta: &MediaDatabaseMetadata) -> error::Result<()> {
        let mut stmt = self.db.prepare("update meta set dirMod = ?")?;
        stmt.execute(params![meta.folder_mtime])?;

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> error::Result<u32> {
        self.db
            .query_row(
                "select count(*) from media where csum is not null",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn all_mtimes(&self) -> error::Result<HashMap<String, i64>> {
        let mut stmt = self
            .db
            .prepare("select fname, mtime from media where csum is not null")?;
        let map: std::result::Result<HashMap<String, i64>, rusqlite::Error> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect();
        Ok(map?)
    }

    /// Returns all filenames and checksums, where the checksum is not null.
    pub(crate) fn all_registered_checksums(&self) -> error::Result<Checksums> {
        self.db
            .prepare("SELECT fname, csum FROM media WHERE csum IS NOT NULL")?
            .query_and_then([], row_to_name_and_checksum)?
            .collect::<error::Result<_>>()
            .map(Checksums)
    }
}

fn row_to_entry(row: &Row) -> rusqlite::Result<MediaEntry> {
    // map the string checksum into bytes
    let sha1_str = row.get_ref(1)?.as_str_or_null()?;
    let sha1_array = if let Some(s) = sha1_str {
        let mut arr = [0; 20];
        match hex::decode_to_slice(s, arr.as_mut()) {
            Ok(_) => Some(arr),
            _ => None,
        }
    } else {
        None
    };
    // and return the entry
    Ok(MediaEntry {
        fname: row.get(0)?,
        sha1: sha1_array,
        mtime: row.get(2)?,
    })
}

fn row_to_name_and_checksum(row: &Row) -> error::Result<(String, Sha1Hash)> {
    let file_name = row.get(0)?;
    let sha1_str: String = row.get(1)?;
    let mut sha1 = [0; 20];
    if let Err(err) = hex::decode_to_slice(sha1_str, &mut sha1) {
        invalid_input!(err, "bad media checksum: {file_name}");
    }
    Ok((file_name, sha1))
}

fn trace(event: rusqlite::trace::TraceEvent) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
        println!("sql: {sql}");
    }
}

pub(crate) fn open_or_create<P: AsRef<Path>>(path: P) -> error::Result<Connection> {
    let mut db = Connection::open(path)?;

    if std::env::var("TRACESQL").is_ok() {
        db.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace),
        );
    }

    db.pragma_update(None, "page_size", 4096)?;
    db.pragma_update(None, "legacy_file_format", false)?;
    db.pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()))?;

    initial_db_setup(&mut db)?;

    Ok(db)
}

fn initial_db_setup(db: &mut Connection) -> error::Result<()> {
    // tables already exist?
    if db
        .prepare_cached("select null from sqlite_master where type = 'table' and name = 'media'")?
        .exists([])?
    {
        return remove_sync_state(db);
    }

    db.execute("begin", [])?;
    db.execute_batch(include_str!("schema.sql"))?;
    db.execute_batch("commit; vacuum; analyze;")?;

    Ok(())
}

/// A database from before sync was removed has a flag per file, tombstones of
/// deleted files, and a sync counter. In one transaction, so it's all or
/// nothing.
fn remove_sync_state(db: &mut Connection) -> error::Result<()> {
    if !db
        .prepare_cached("select null from pragma_table_info('media') where name = 'dirty'")?
        .exists([])?
    {
        return Ok(());
    }
    let transaction = db.transaction()?;
    transaction.execute_batch(
        "DROP INDEX IF EXISTS idx_media_dirty;
         ALTER TABLE media DROP COLUMN dirty;
         ALTER TABLE meta DROP COLUMN lastUsn;
         DELETE FROM media WHERE csum IS NULL;",
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use anki_io::new_tempfile;
    use rusqlite::Connection;
    use tempfile::TempDir;

    use crate::error::Result;
    use crate::media::database::MediaEntry;
    use crate::media::files::sha1_of_data;
    use crate::media::MediaManager;

    #[test]
    fn database() -> Result<()> {
        let folder = TempDir::new()?;
        let db_file = new_tempfile()?;
        let db_file_path = db_file.path().to_str().unwrap();
        let mut mgr = MediaManager::new(folder.path(), db_file_path)?;
        mgr.db.transact(|ctx| {
            // no entry exists yet
            assert_eq!(ctx.get_entry("test.mp3")?, None);

            // add one
            let mut entry = MediaEntry {
                fname: "test.mp3".into(),
                sha1: None,
                mtime: 0,
            };
            ctx.set_entry(&entry)?;
            assert_eq!(ctx.get_entry("test.mp3")?.unwrap(), entry);

            // update it
            entry.sha1 = Some(sha1_of_data(b"hello"));
            entry.mtime = 123;
            ctx.set_entry(&entry)?;
            assert_eq!(ctx.get_entry("test.mp3")?.unwrap(), entry);

            let mut meta = ctx.get_meta()?;
            assert_eq!(meta.folder_mtime, 0);

            meta.folder_mtime = 123;

            ctx.set_meta(&meta)?;

            meta = ctx.get_meta()?;
            assert_eq!(meta.folder_mtime, 123);

            Ok(())
        })?;

        // reopen database and ensure data was committed
        drop(mgr);
        mgr = MediaManager::new(folder.path(), db_file_path)?;
        let meta = mgr.db.get_meta()?;
        assert_eq!(meta.folder_mtime, 123);

        Ok(())
    }

    /// A media database from before keeps what it knows about the files, and
    /// loses what was only for sync, however many times it's opened.
    #[test]
    fn old_database_loses_its_sync_state() -> Result<()> {
        let folder = TempDir::new()?;
        let db_file = new_tempfile()?;
        let sha1 = hex::encode(sha1_of_data(b"hello"));
        {
            let db = Connection::open(db_file.path())?;
            db.execute_batch(&format!(
                "CREATE TABLE media (fname text NOT NULL PRIMARY KEY, csum text,
                   mtime int NOT NULL, dirty int NOT NULL) without rowid;
                 CREATE INDEX idx_media_dirty ON media (dirty) WHERE dirty = 1;
                 CREATE TABLE meta (dirMod int, lastUsn int);
                 INSERT INTO meta VALUES (55, 66);
                 INSERT INTO media VALUES ('kept.jpg', '{sha1}', 77, 1);
                 INSERT INTO media VALUES ('deleted.jpg', NULL, 0, 1);"
            ))?;
        }
        for _ in 0..2 {
            let mgr = MediaManager::new(folder.path(), db_file.path())?;
            assert_eq!(mgr.db.get_meta()?.folder_mtime, 55);
            assert_eq!(mgr.db.get_entry("deleted.jpg")?, None);
            assert_eq!(
                mgr.db.get_entry("kept.jpg")?,
                Some(MediaEntry {
                    fname: "kept.jpg".into(),
                    sha1: Some(sha1_of_data(b"hello")),
                    mtime: 77,
                })
            );
        }
        let db = Connection::open(db_file.path())?;
        let columns: String = db.query_row(
            "select group_concat(name) from (select name from pragma_table_info('media')
             union all select name from pragma_table_info('meta'))",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(columns, "fname,csum,mtime,dirMod");
        Ok(())
    }
}
