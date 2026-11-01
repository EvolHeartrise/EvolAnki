// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Automatic backups: a consistent copy of the collection in a `backups` folder
//! beside it, at start, every so often while it's changing, and at shutdown.
//! There is no UI. A failure says so loudly, and never gets in the way of
//! reviewing. docs/web-server.md has the retention, what's not covered, and how
//! to restore.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use anki::backend::Backend;
use anki::storage::copy_database;
use anki::storage::verify_copied_collection;
use anyhow::Context;
use anyhow::Result;
use chrono::Local;
use chrono::NaiveDateTime;
use rusqlite::Connection;
use rusqlite::OpenFlags;

/// How many of the latest backups to keep, whatever their age.
const KEEP_RECENT: usize = 48;
/// Older than those, the last backup of each of this many days.
const KEEP_DAILY: usize = 30;

const TIME_FORMAT: &str = "%Y-%m-%d-%H.%M.%S";
const TEMP_SUFFIX: &str = ".tmp";

type Stamp = (i64, i64, i64);

pub struct Backups {
    folder: PathBuf,
    /// The collection's filename without its extension, which starts each
    /// backup's.
    stem: String,
    last_stamp: Mutex<Option<Stamp>>,
}

impl Backups {
    pub fn new(collection: &Path) -> Result<Self> {
        let parent = collection.parent().filter(|p| !p.as_os_str().is_empty());
        let stem = collection
            .file_stem()
            .context("collection path has no filename")?
            .to_str()
            .context("collection filename is not UTF-8")?
            .to_string();
        Ok(Self {
            folder: parent.unwrap_or(Path::new(".")).join("backups"),
            stem,
            last_stamp: Mutex::new(None),
        })
    }

    /// Back up a collection nothing has open yet.
    pub fn take_of_closed_file(&self, collection: &Path) {
        let result = self.write(|destination| {
            let source = Connection::open_with_flags(collection, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            copy_database(&source, destination)?;
            Ok(())
        });
        self.report(result);
    }

    /// Back up the open collection, however long the backend is busy for.
    pub fn take(&self, backend: &Backend) {
        self.report(self.try_take(backend));
    }

    /// The same, for callers that must not go on without one.
    pub fn try_take(&self, backend: &Backend) -> Result<PathBuf> {
        let mut stamp = None;
        let result = self.write(|destination| {
            backend.with_collection(|col| {
                stamp = Some(col.storage.change_stamp()?);
                copy_database(col.storage.db(), destination)
            })?;
            Ok(())
        });
        if result.is_ok() {
            *self.last_stamp.lock().unwrap() = stamp;
        }
        result
    }

    /// Where these go.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Treat the collection as it is now as already backed up.
    pub fn remember_state(&self, backend: &Backend) {
        match stamp(backend) {
            Ok(stamp) => *self.last_stamp.lock().unwrap() = Some(stamp),
            Err(err) => shout(&err),
        }
    }

    fn changed(&self, backend: &Backend) -> bool {
        match stamp(backend) {
            Ok(stamp) => *self.last_stamp.lock().unwrap() != Some(stamp),
            Err(err) => {
                shout(&err);
                false
            }
        }
    }

    /// Writes under a temporary name, so that a backup with its final name is
    /// always complete, even after a crash or full disk.
    fn write(&self, copy: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
        fs::create_dir_all(&self.folder).with_context(|| self.folder.display().to_string())?;
        let final_path = self.unused_path();
        let temp = PathBuf::from(format!("{}{TEMP_SUFFIX}", final_path.display()));
        let result = (|| {
            copy(&temp)?;
            verify_copied_collection(&temp)?;
            fs::File::open(&temp)?.sync_all()?;
            fs::rename(&temp, &final_path)?;
            sync_folder(&self.folder);
            Ok(final_path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        self.remove_old()?;
        result
    }

    fn unused_path(&self) -> PathBuf {
        let time = Local::now().format(TIME_FORMAT);
        let mut path = self.folder.join(format!("{}-{time}.anki2", self.stem));
        // A second backup within the same second; both are of interest.
        let mut attempt = 1;
        while path.exists() {
            attempt += 1;
            path = self
                .folder
                .join(format!("{}-{time}-{attempt}.anki2", self.stem));
        }
        path
    }

    fn remove_old(&self) -> Result<()> {
        let names: Vec<String> = fs::read_dir(&self.folder)?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        for name in names_to_remove(&self.stem, &names) {
            fs::remove_file(self.folder.join(&name)).with_context(|| name)?;
        }
        Ok(())
    }

    fn report(&self, result: Result<PathBuf>) {
        match result {
            Ok(path) => tracing::info!("backed up to {}", path.display()),
            Err(err) => shout(&err),
        }
    }
}

fn stamp(backend: &Backend) -> Result<Stamp> {
    Ok(backend.with_collection(|col| col.storage.change_stamp())?)
}

/// Not every failure can be fixed from here, but none should go unnoticed.
fn shout(err: &anyhow::Error) {
    eprintln!("BACKUP FAILED, reviews are not being backed up: {err:#}");
}

#[cfg(unix)]
fn sync_folder(folder: &Path) {
    if let Ok(folder) = fs::File::open(folder) {
        let _ = folder.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_folder(_folder: &Path) {}

/// From `col-2026-11-01-05.30.12.anki2` or `col-2026-11-01-05.30.12-2.anki2`.
fn parse_time(stem: &str, name: &str) -> Option<NaiveDateTime> {
    let rest = name.strip_prefix(stem)?.strip_prefix('-')?;
    let rest = rest.strip_suffix(".anki2")?;
    let time = rest.get(..19)?;
    let extra = &rest[19..];
    if !(extra.is_empty() || extra.starts_with('-')) {
        return None;
    }
    NaiveDateTime::parse_from_str(time, TIME_FORMAT).ok()
}

/// Backups beyond the limits, and temporary files that a crash left behind.
/// Anything else in the folder is someone else's.
fn names_to_remove(stem: &str, names: &[String]) -> Vec<String> {
    let mut temps = names
        .iter()
        .filter(|name| {
            name.strip_suffix(TEMP_SUFFIX)
                .is_some_and(|name| parse_time(stem, name).is_some())
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut backups: Vec<(NaiveDateTime, &String)> = names
        .iter()
        .filter_map(|name| Some((parse_time(stem, name)?, name)))
        .collect();
    backups.sort();
    backups.reverse();

    let mut keep: HashSet<&String> = backups.iter().take(KEEP_RECENT).map(|b| b.1).collect();
    let mut days = HashSet::new();
    for (time, name) in &backups {
        if days.len() == KEEP_DAILY && !days.contains(&time.date()) {
            break;
        }
        if days.insert(time.date()) {
            keep.insert(name);
        }
    }

    temps.extend(
        backups
            .iter()
            .filter(|(_, name)| !keep.contains(name))
            .map(|(_, name)| (*name).clone()),
    );
    temps
}

pub async fn run_periodically(backups: Arc<Backups>, backend: Backend, interval: Duration) {
    let mut last = Instant::now();
    let mut ticker = tokio::time::interval(interval.min(Duration::from_secs(60)));
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if last.elapsed() < interval {
            continue;
        }
        if take_if_changed_blocking(backups.clone(), backend.clone()).await {
            last = Instant::now();
        }
    }
}

/// Whether it took one. A panic doesn't get past here either.
pub async fn take_if_changed_blocking(backups: Arc<Backups>, backend: Backend) -> bool {
    let result = tokio::task::spawn_blocking(move || {
        let changed = backups.changed(&backend);
        if changed {
            backups.take(&backend);
        }
        changed
    })
    .await;
    match result {
        Ok(taken) => taken,
        Err(err) => {
            shout(&anyhow::anyhow!("{err}"));
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn name(day: u32, hour: u32) -> String {
        format!(
            "col-2026-{:02}-{:02}-{hour:02}.00.00.anki2",
            1 + day / 28,
            1 + day % 28
        )
    }

    #[test]
    fn names_are_understood() {
        assert!(parse_time("col", "col-2026-11-01-05.30.12.anki2").is_some());
        assert!(parse_time("col", "col-2026-11-01-05.30.12-2.anki2").is_some());
        for other in [
            "col-2026-11-01-05.30.12.anki2.tmp",
            "col.anki2",
            "other-2026-11-01-05.30.12.anki2",
            "col-notes.anki2",
            "col-2026-11-01-05.30.12x.anki2",
            "backup-2026-11-01-05.30.12.colpkg",
        ] {
            assert!(parse_time("col", other).is_none(), "{other}");
        }
    }

    #[test]
    fn nothing_goes_while_under_the_limits() {
        let names: Vec<_> = (0..KEEP_RECENT as u32).map(|h| name(h, 0)).collect();
        assert!(names_to_remove("col", &names).is_empty());
    }

    #[test]
    fn keeps_the_recent_ones_and_one_for_each_of_the_last_days() {
        // 100 days, each with 24 backups
        let mut names = vec![];
        for day in 0..100 {
            for hour in 0..24 {
                names.push(name(day, hour));
            }
        }
        names.push("unrelated.txt".to_string());
        let removed: HashSet<_> = names_to_remove("col", &names).into_iter().collect();
        let kept: Vec<_> = names.iter().filter(|n| !removed.contains(*n)).collect();
        assert!(kept.iter().any(|n| *n == "unrelated.txt"));
        let kept: Vec<_> = kept.into_iter().filter(|n| *n != "unrelated.txt").collect();

        // the latest two days' worth, and the rest of the 30 days' last ones
        assert_eq!(kept.len(), KEEP_RECENT + KEEP_DAILY - 2);
        assert!(kept.contains(&&name(99, 23)));
        assert!(kept.contains(&&name(99, 0)));
        assert!(kept.contains(&&name(98, 0)));
        assert!(!kept.contains(&&name(97, 22)));
        assert!(kept.contains(&&name(97, 23)));
        assert!(kept.contains(&&name(70, 23)));
        assert!(!kept.contains(&&name(69, 23)));
    }

    #[test]
    fn restarts_on_one_day_cant_push_out_older_days() {
        let mut names: Vec<_> = (0..500)
            .map(|i| format!("col-2026-11-02-{:02}.{:02}.00.anki2", i / 60, i % 60))
            .collect();
        names.push("col-2026-11-01-23.00.00.anki2".to_string());
        let removed = names_to_remove("col", &names);
        assert!(!removed.contains(&"col-2026-11-01-23.00.00.anki2".to_string()));
        assert_eq!(removed.len(), 500 - KEEP_RECENT);
    }

    #[test]
    fn leftover_temporary_files_go() {
        let names = vec![
            "col-2026-11-01-05.30.12.anki2.tmp".to_string(),
            "mine.tmp".to_string(),
        ];
        assert_eq!(
            names_to_remove("col", &names),
            vec!["col-2026-11-01-05.30.12.anki2.tmp".to_string()]
        );
    }
}
