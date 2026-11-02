// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Check a converted collection against the snapshot of its source, and write
//! a report for its owner. The target is only read: anything that goes through
//! the backend, including rendering, the media check and the optimizer, runs on
//! a throwaway copy.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use anki::backend::Backend;
use anki::storage::QUEUE_ONCE_OPENED_SQL;
use anki_proto::card_rendering::RenderCardResponse;
use anki_proto::card_rendering::RenderExistingCardRequest;
use anki_proto::cards::CardId;
use anki_proto::collection::CheckDatabaseResponse;
use anki_proto::deck_config::deck_config::Config;
use anki_proto::deck_config::DeckConfig;
use anki_proto::deck_config::DeckConfigId;
use anki_proto::deck_config::DeckConfigsForUpdate;
use anki_proto::decks::deck;
use anki_proto::decks::deck::Kind;
use anki_proto::decks::Deck;
use anki_proto::decks::DeckId;
use anki_proto::generic;
use anki_proto::media::CheckMediaResponse;
use anki_proto::scheduler::ComputeFsrsParamsRequest;
use anki_proto::scheduler::ComputeFsrsParamsResponse;
use anki_proto::scheduler::EvaluateParamsLegacyRequest;
use anki_proto::scheduler::EvaluateParamsResponse;
use anki_proto::scheduler::SchedTimingTodayResponse;
use anki_proto::stats::CardStatsResponse;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use percent_encoding::percent_decode_str;
use prost::Message;
use regex::Regex;
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

use super::commands::deck_rows_today;
use super::commands::flatten;
use super::convert::hash_folder;
use super::convert::hash_source;
use super::convert::manifest_path;
use super::convert::open_immutable;
use super::convert::snapshot_path;
use super::socket::socket_path;
use crate::collection::close_collection;
use crate::collection::media_paths;
use crate::collection::open_collection;
use crate::rpc;
use crate::rpc::call_typed;

/// Formats iOS or browsers in general may not play or show.
const RISKY_FORMATS: &[(&str, &str)] = &[
    ("ogg", "no iOS Safari before 18.4"),
    ("oga", "no iOS Safari before 18.4"),
    ("opus", "no iOS Safari before 18.4"),
    ("spx", "no browser plays Speex"),
    ("wma", "no browser plays it"),
    ("flac", "fine on current browsers; none before iOS 11"),
    ("webm", "iOS Safari only recently, and patchy"),
    ("mkv", "not reliable anywhere"),
    ("avi", "not reliable anywhere"),
    ("mpg", "not reliable anywhere"),
    ("mpeg", "not reliable anywhere"),
    ("wmv", "no browser plays it"),
    ("tif", "only Safari shows it"),
    ("tiff", "only Safari shows it"),
    ("heic", "only Safari shows it"),
];

struct Out(String);

impl Out {
    fn line(&mut self, text: impl AsRef<str>) {
        self.0.push_str(text.as_ref());
        self.0.push('\n');
    }

    fn head(&mut self, text: &str) {
        let _ = write!(self.0, "\n## {text}\n\n");
    }

    fn table(&mut self, headers: &[&str], rows: &[Vec<String>]) {
        self.line(format!("| {} |", headers.join(" | ")));
        self.line(format!(
            "|{}",
            headers.iter().map(|_| "---|").collect::<String>()
        ));
        for row in rows {
            self.line(format!("| {} |", row.join(" | ").replace('\n', " ")));
        }
        self.line("");
    }
}

fn scalar(db: &Connection, sql: &str) -> Result<i64> {
    Ok(db.query_row(sql, [], |row| row.get(0))?)
}

/// Every row of a table, keyed by its first column, as text.
fn rows_by_id(db: &Connection, sql: &str) -> Result<BTreeMap<i64, Vec<Sql>>> {
    let mut statement = db.prepare(sql)?;
    let width = statement.column_count();
    let rows = statement.query_map([], |row| {
        let mut values = Vec::with_capacity(width - 1);
        for index in 1..width {
            values.push(row.get::<_, Sql>(index)?);
        }
        Ok((row.get::<_, i64>(0)?, values))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// (only in a, only in b, differing), per column for those that differ.
fn compare(
    a: &BTreeMap<i64, Vec<Sql>>,
    b: &BTreeMap<i64, Vec<Sql>>,
    columns: &[&str],
) -> (usize, usize, BTreeMap<String, usize>) {
    let only_a = a.keys().filter(|id| !b.contains_key(id)).count();
    let only_b = b.keys().filter(|id| !a.contains_key(id)).count();
    let mut per_column = BTreeMap::new();
    for (id, left) in a {
        if let Some(right) = b.get(id) {
            for (index, column) in columns.iter().enumerate() {
                if left[index] != right[index] {
                    *per_column.entry(column.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    (only_a, only_b, per_column)
}

fn describe_comparison(result: &(usize, usize, BTreeMap<String, usize>)) -> String {
    let (only_a, only_b, columns) = result;
    if *only_a == 0 && *only_b == 0 && columns.is_empty() {
        return "identical".to_string();
    }
    format!("**DIFFERENT**: {only_a} only before, {only_b} only after, per column: {columns:?}")
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn distribution(values: &mut [f64]) -> Vec<String> {
    values.sort_by(|a, b| a.total_cmp(b));
    [0.0, 0.05, 0.25, 0.5, 0.75, 0.95, 1.0]
        .iter()
        .map(|p| format!("{:.3}", percentile(values, *p)))
        .collect()
}

struct Refs {
    sound: Regex,
    image: Regex,
    av: Regex,
    css_url: Regex,
    tag: Regex,
    script: Regex,
    handler: Regex,
    remote: Regex,
    field_ref: Regex,
}

impl Refs {
    fn new() -> Result<Self> {
        Ok(Self {
            sound: Regex::new(r"\[sound:([^\]]+)\]")?,
            image: Regex::new(r#"(?is)<img\b[^>]*?\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)?,
            av: Regex::new(
                r#"(?is)<(?:audio|video|source|object|embed)\b[^>]*?\b(?:src|data)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,
            )?,
            css_url: Regex::new(r#"(?i)url\(\s*["']?([^"')]+)["']?\s*\)"#)?,
            tag: Regex::new(r"(?s)<[^>]*>")?,
            script: Regex::new(r"(?i)<script\b")?,
            handler: Regex::new(r#"(?i)<[^>]*\son[a-z]+\s*="#)?,
            remote: Regex::new(
                r#"(?i)(?:src|href|data)\s*=\s*["']?(?:https?:)?//[^\s"'>]+|url\(\s*["']?(?:https?:)?//[^)]+|@import\s+[^;]*(?:https?:)?//[^;]+"#,
            )?,
            field_ref: Regex::new(r"\{\{([^{}]+)\}\}")?,
        })
    }

    /// References to files, as they're stored in media: entity and percent
    /// encoding undone.
    fn files(&self, html: &str, out: &mut BTreeSet<String>, remote: &mut BTreeSet<String>) {
        for caps in self.sound.captures_iter(html) {
            out.insert(decode_entities(&caps[1]));
        }
        for regex in [&self.image, &self.av] {
            for caps in regex.captures_iter(html) {
                let src = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .or_else(|| caps.get(3))
                    .map_or("", |m| m.as_str());
                let src = decode_entities(src);
                let lower = src.to_lowercase();
                if lower.starts_with("http:")
                    || lower.starts_with("https:")
                    || lower.starts_with("//")
                {
                    remote.insert(src);
                } else if !lower.starts_with("data:") && !src.is_empty() {
                    out.insert(
                        percent_decode_str(&src)
                            .decode_utf8()
                            .map(|s| s.into_owned())
                            .unwrap_or(src),
                    );
                }
            }
        }
    }
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", "\u{a0}")
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn sample(items: impl IntoIterator<Item = impl ToString>, limit: usize) -> String {
    let items: Vec<String> = items.into_iter().map(|i| i.to_string()).collect();
    let mut shown = items
        .iter()
        .take(limit)
        .map(|i| format!("`{}`", i.replace('`', "'").replace('|', "\\|")))
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > limit {
        let _ = write!(shown, ", and {} more", items.len() - limit);
    }
    shown
}

pub fn validate(target: &Path, report_path: &Path, work_dir: &Path) -> Result<Value> {
    let target = target
        .canonicalize()
        .with_context(|| target.display().to_string())?;
    if std::os::unix::net::UnixStream::connect(socket_path(&target)).is_ok() {
        bail!("a server has this collection open; stop it first");
    }
    let manifest: Value = serde_json::from_slice(
        &fs::read(manifest_path(&target)).context("no manifest; was this made by convert?")?,
    )?;
    let snapshot_file = snapshot_path(&target);
    let snapshot = open_immutable(&snapshot_file)?;
    let converted = open_immutable(&target)?;

    let work = work_dir.join(format!("evolanki-validate-{}", std::process::id()));
    fs::create_dir_all(&work)?;
    let result = validate_with_work_dir(
        &target,
        report_path,
        &work,
        &manifest,
        &snapshot,
        &converted,
    );
    let _ = fs::remove_dir_all(&work);
    result
}

fn validate_with_work_dir(
    target: &Path,
    report_path: &Path,
    work: &Path,
    manifest: &Value,
    snapshot: &Connection,
    converted: &Connection,
) -> Result<Value> {
    let mut out = Out(String::new());
    let mut problems: Vec<String> = vec![];
    out.line("# Conversion report");
    out.line("");
    out.line(format!(
        "Written {} by `evolanki-admin validate`. Compares `{}` against a snapshot of `{}` taken \
         by the SQLite backup API through a read-only URI.",
        chrono::Local::now().format("%Y-%m-%d %H:%M %Z"),
        target.display(),
        manifest["source"].as_str().unwrap_or_default(),
    ));

    // the source
    out.head("Source unchanged");
    let source = Path::new(manifest["source"].as_str().context("manifest source")?);
    let (source_media, source_media_db) = media_paths(source);
    let now = hash_source(source, &source_media, &source_media_db)?;
    let before = &manifest["source_hashes_before"];
    let mut rows = vec![];
    for key in ["collection", "wal", "shm", "media_db", "media_folder"] {
        rows.push(vec![
            key.to_string(),
            before[key]["sha256"].as_str().unwrap_or("-").to_string(),
            now[key]["sha256"].as_str().unwrap_or("-").to_string(),
            (before[key] == now[key]).to_string(),
        ]);
    }
    out.table(
        &["file", "sha256 before conversion", "sha256 now", "same"],
        &rows,
    );
    if *before != now {
        problems.push("the source's files changed since conversion".to_string());
    }
    out.line(format!(
        "Snapshot pages byte-identical to the source's, apart from the 100-byte header: {}.",
        manifest["snapshot_pages_identical_to_source"]
    ));

    // counts
    out.head("Counts");
    let mut rows = vec![];
    for table in [
        "notes",
        "cards",
        "revlog",
        "decks",
        "notetypes",
        "deck_config",
        "fields",
        "templates",
        "tags",
        "config",
    ] {
        let sql = format!("select count() from {table}");
        let (a, b) = (scalar(snapshot, &sql)?, scalar(converted, &sql)?);
        // each normal deck has its own row, whatever the source's decks shared;
        // the deck settings section compares what they use
        if a != b && table != "deck_config" {
            problems.push(format!("{table} count changed from {a} to {b}"));
        }
        rows.push(vec![table.to_string(), a.to_string(), b.to_string()]);
    }
    out.table(&["table", "before", "after"], &rows);
    if let Ok(graves) = scalar(snapshot, "select count() from graves") {
        out.line(format!(
            "The source's `graves` table, {graves} rows, and its `usn`, `scm`, `dty` and `ls` \
             columns only tracked sync, so they aren't in the converted collection."
        ));
    }
    let orphans = "select count() from revlog where cid not in (select id from cards)";
    out.line(format!(
        "Revlog rows whose card no longer exists (its note was deleted, and desktop Anki keeps \
         the history): {}. They count in the all-time stats, but the optimizer finds reviews by \
         card, so doesn't use them.",
        scalar(snapshot, orphans)?
    ));

    // notes and revlog
    out.head("Notes and revlog");
    let hash_notes = |db: &Connection| -> Result<BTreeMap<i64, Vec<Sql>>> {
        let mut statement = db.prepare("select id, flds, tags, guid, mid, mod from notes")?;
        let rows = statement.query_map([], |row| {
            let mut hasher = Sha256::new();
            hasher.update(row.get::<_, String>(1)?.as_bytes());
            hasher.update([0]);
            hasher.update(row.get::<_, String>(2)?.as_bytes());
            Ok((
                row.get::<_, i64>(0)?,
                vec![
                    Sql::Text(hex::encode(hasher.finalize())),
                    row.get::<_, Sql>(3)?,
                    row.get::<_, Sql>(4)?,
                    row.get::<_, Sql>(5)?,
                ],
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let note_result = compare(
        &hash_notes(snapshot)?,
        &hash_notes(converted)?,
        &["fields and tags hash", "guid", "notetype", "mod"],
    );
    out.line(format!(
        "- Per-note SHA-256 of fields and tags, plus guid, notetype and mod: {}",
        describe_comparison(&note_result)
    ));
    let revlog_sql = "select id, cid, ease, ivl, lastIvl, factor, time, type from revlog";
    let revlog_result = compare(
        &rows_by_id(snapshot, revlog_sql)?,
        &rows_by_id(converted, revlog_sql)?,
        &["cid", "ease", "ivl", "lastIvl", "factor", "time", "type"],
    );
    out.line(format!(
        "- Revlog, every column but `usn` of every row: {}",
        describe_comparison(&revlog_result)
    ));

    // scheduling
    out.head("Scheduling");
    let card_sql = "select id, due, ivl, queue, type, reps, lapses, factor, left, odue, odid, \
                    flags, did, ord, nid, mod, data from cards";
    let card_columns = [
        "due", "ivl", "queue", "type", "reps", "lapses", "factor", "left", "odue", "odid", "flags",
        "did", "ord", "nid", "mod", "data",
    ];
    // opening a collection returns buried cards to normal, which isn't a loss
    let cards_before = rows_by_id(
        snapshot,
        &card_sql.replacen("queue", QUEUE_ONCE_OPENED_SQL, 1),
    )?;
    let cards_after = rows_by_id(converted, card_sql)?;
    let card_result = compare(&cards_before, &cards_after, &card_columns);
    out.line(format!(
        "Every card, due, ivl, queue, type, reps, lapses, and all other columns: {}",
        describe_comparison(&card_result)
    ));
    if !card_result.2.is_empty() || note_result.0 + note_result.1 + revlog_result.0 > 0 {
        problems.push("notes, cards or revlog differ".to_string());
    }
    out.line(format!(
        "Cards in queue -2 or -3 in the source, now normal: {}",
        scalar(
            snapshot,
            "select count() from cards where queue in (-2, -3)"
        )?
    ));
    out.line("");
    let rows: Vec<Vec<String>> = {
        let mut statement = snapshot.prepare(
            "select queue, type, count() from cards group by queue, type order by queue, type",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(vec![
                row.get::<_, i64>(0)?.to_string(),
                row.get::<_, i64>(1)?.to_string(),
                row.get::<_, i64>(2)?.to_string(),
            ])
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    out.table(&["queue", "type", "cards"], &rows);

    // memory state
    out.head("Memory state");
    let data_of = |row: &Vec<Sql>| -> Value {
        match row.last() {
            Some(Sql::Text(text)) => serde_json::from_str(text).unwrap_or(Value::Null),
            _ => Value::Null,
        }
    };
    let (mut had, mut kept, mut altered, mut gained, mut none_new, mut none_other) =
        (0, 0, 0, 0, 0, vec![]);
    let (mut stabilities, mut difficulties) = (vec![], vec![]);
    let (mut no_decay, mut no_last_review) = (0, 0);
    let mut retentions: BTreeMap<String, usize> = BTreeMap::new();
    for (id, after) in &cards_after {
        let (data_after, data_before) = (
            data_of(after),
            cards_before.get(id).map(data_of).unwrap_or(Value::Null),
        );
        let has = |data: &Value| data.get("s").is_some();
        match (has(&data_before), has(&data_after)) {
            (true, true)
                if data_before["s"] == data_after["s"] && data_before["d"] == data_after["d"] =>
            {
                had += 1;
                kept += 1;
            }
            (true, _) => {
                had += 1;
                altered += 1;
            }
            (false, true) => gained += 1,
            (false, false) => {
                // type is the 4th column after the id
                let ctype = matches!(after[3], Sql::Integer(0));
                if ctype {
                    none_new += 1;
                } else {
                    none_other.push(*id);
                }
            }
        }
        if has(&data_after) {
            stabilities.push(data_after["s"].as_f64().unwrap_or(f64::NAN));
            difficulties.push(data_after["d"].as_f64().unwrap_or(f64::NAN));
            if data_after.get("decay").is_none() {
                no_decay += 1;
            }
            if data_after.get("lrt").is_none() {
                no_last_review += 1;
            }
            *retentions
                .entry(
                    data_after
                        .get("dr")
                        .map_or("none".into(), |d| d.to_string()),
                )
                .or_default() += 1;
        }
    }
    out.line(format!(
        "- Had a memory state before: {had}; unchanged: {kept}; altered: {altered}.\n\
         - Gained one in conversion: {gained}.\n\
         - None, as they're new and have never been answered, which is how it should be: {none_new}.\n\
         - None although reviewed: {} {}",
        none_other.len(),
        sample(&none_other, 10)
    ));
    if altered > 0 || !none_other.is_empty() {
        problems.push(format!(
            "memory state: {altered} altered, {} reviewed cards have none",
            none_other.len()
        ));
    }
    out.line("");
    let mut rows = vec![];
    for (name, values) in [
        ("stability (days)", &mut stabilities),
        ("difficulty (1-10)", &mut difficulties),
    ] {
        let mut row = vec![name.to_string()];
        row.extend(distribution(values));
        rows.push(row);
    }
    out.table(&["", "min", "5%", "25%", "50%", "75%", "95%", "max"], &rows);
    let buckets = [0.0, 1.0, 3.0, 7.0, 30.0, 90.0, 365.0, f64::MAX];
    let rows: Vec<Vec<String>> = buckets
        .windows(2)
        .map(|w| {
            vec![
                format!(
                    "{} to {}",
                    w[0],
                    if w[1] == f64::MAX {
                        "more".into()
                    } else {
                        w[1].to_string()
                    }
                ),
                stabilities
                    .iter()
                    .filter(|s| **s >= w[0] && **s < w[1])
                    .count()
                    .to_string(),
            ]
        })
        .collect();
    out.table(&["stability, days", "cards"], &rows);
    let out_of_range = difficulties
        .iter()
        .filter(|d| !(1.0..=10.0).contains(*d))
        .count();
    out.line(format!(
        "Difficulty out of 1-10: {out_of_range}. Stored desired retention: {retentions:?}. Without a \
         stored decay, so read as FSRS 5's 0.5 until next answered: {no_decay}. Without a stored last \
         review time, so worked out from due and interval: {no_last_review}."
    ));

    // throwaway copy for anything through the backend
    let throwaway = work.join("collection.anki2");
    fs::copy(target, &throwaway)?;
    let (target_media, _) = media_paths(target);
    let (throwaway_media, _) = media_paths(&throwaway);
    let copied = Command::new("cp")
        .args(["-a", "--"])
        .arg(&target_media)
        .arg(&throwaway_media)
        .status()?;
    if !copied.success() {
        bail!("couldn't copy media for checking");
    }
    let backend = rpc::new_backend()?;
    open_collection(&backend, &throwaway)?;

    let parts = decks_section(&mut out, &backend, snapshot, &mut problems)?;
    due_section(&mut out, &backend, snapshot, &mut problems)?;
    rendering_section(&mut out, &backend, snapshot, &target_media, &mut problems)?;
    media_section(&mut out, &backend, target, manifest, &mut problems)?;
    settings_section(&mut out, snapshot, converted, &parts)?;
    retrievability_section(&mut out, &backend, snapshot)?;
    let checked: CheckDatabaseResponse = call_typed(&backend, "checkDatabase", &generic::Empty {})?;
    out.head("Database check");
    if checked.problems.is_empty() {
        out.line("The backend's database check, on a throwaway copy, found nothing to fix.");
    } else {
        out.line(
            "The backend's database check, on a throwaway copy, would do this. Nothing has been \
             done to the target. Adding last review times is a repair only of a cached value.",
        );
        out.line("");
        for problem in &checked.problems {
            out.line(format!("- {problem}"));
        }
        // A newer field that Anki's older versions never filled in. It's worked
        // out from due and interval until then, which gives the same answer.
        let serious: Vec<_> = checked
            .problems
            .iter()
            .filter(|problem| !problem.contains("last review time"))
            .collect();
        if !serious.is_empty() {
            problems.push(format!("the database check found problems: {serious:?}"));
        }
    }
    close_collection(&backend)?;

    // settle what the owner should look at
    let mut summary = Out(String::new());
    summary.head("Summary");
    if problems.is_empty() {
        summary.line("No problems were found in what this can check. Read the notes per section.");
    } else {
        for problem in &problems {
            summary.line(format!("- **{problem}**"));
        }
    }
    let text = out.0.replacen("\n## ", &format!("{}\n## ", summary.0), 1);
    fs::write(report_path, text)?;
    Ok(json!({"report": report_path, "problems": problems}))
}

struct DeckSettingsInfo {
    easy_days: Vec<(String, Vec<f32>)>,
}

/// What a deck had in the snapshot, by the rules from before settings belonged
/// to decks: the row it pointed at, or row 1, with its overrides.
fn settings_in_snapshot(snapshot: &Connection, did: i64) -> Result<Option<Config>> {
    let kind: Vec<u8> =
        snapshot.query_row("select kind from decks where id = ?", [did], |row| {
            row.get(0)
        })?;
    let Some(deck::kind_container::Kind::Normal(normal)) =
        deck::KindContainer::decode(kind.as_slice())?.kind
    else {
        return Ok(None);
    };
    let row = |dcid: i64| {
        snapshot
            .query_row(
                "select config from deck_config where id = ?",
                [dcid],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
    };
    let bytes = match row(normal.config_id)? {
        Some(bytes) => Some(bytes),
        None => row(1)?,
    };
    let mut config = match bytes {
        Some(bytes) => Config::decode(bytes.as_slice())?,
        None => return Ok(None),
    };
    if let Some(limit) = normal.review_limit {
        config.reviews_per_day = limit;
    }
    if let Some(limit) = normal.new_limit {
        config.new_per_day = limit;
    }
    if let Some(retention) = normal.desired_retention {
        config.desired_retention = retention;
    }
    Ok(Some(config))
}

fn decks_section(
    out: &mut Out,
    backend: &Backend,
    snapshot: &Connection,
    problems: &mut Vec<String>,
) -> Result<DeckSettingsInfo> {
    out.head("FSRS parameters per deck");
    let deck_ids: Vec<i64> = {
        let mut statement = snapshot.prepare("select id from decks order by name")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut rows = vec![];
    let mut easy_days = vec![];
    let mut optimizer_rows = vec![];
    let mut changed = vec![];
    for did in deck_ids {
        let deck: Deck = call_typed(backend, "getDeck", &DeckId { did })?;
        if !matches!(deck.kind, Some(Kind::Normal(_))) {
            continue;
        }
        let name = deck.name;
        let settings: DeckConfig =
            call_typed(backend, "getDeckConfig", &DeckConfigId { dcid: did })?;
        let config = settings.config.clone().context("deck has no settings")?;
        if settings_in_snapshot(snapshot, did)?
            .as_ref()
            .is_some_and(|before| before != &config)
        {
            changed.push(name.clone());
        }
        let (version, params) = if !config.fsrs_params_6.is_empty() {
            ("FSRS 6", config.fsrs_params_6.clone())
        } else if !config.fsrs_params_5.is_empty() {
            ("FSRS 5", config.fsrs_params_5.clone())
        } else if !config.fsrs_params_4.is_empty() {
            ("FSRS 4", config.fsrs_params_4.clone())
        } else {
            ("none, defaults apply", vec![])
        };
        rows.push(vec![
            name.clone(),
            format!("{version} ({} values)", params.len()),
            format!("{:.2}", config.desired_retention),
            format!("{:.2}", config.historical_retention),
            config.new_per_day.to_string(),
            config.reviews_per_day.to_string(),
            config.maximum_review_interval.to_string(),
            format!("`{}`", config.ignore_revlogs_before_date),
            format!("`{}`", config.param_search),
        ]);
        easy_days.push((name.clone(), config.easy_days_percentages.clone()));

        // what the optimizer would say, on the throwaway copy
        let search = if config.param_search.is_empty() {
            let info: DeckConfigsForUpdate =
                call_typed(backend, "getDeckConfigsForUpdate", &DeckId { did })?;
            info.current_deck.context("no current deck")?.default_search
        } else {
            config.param_search.clone()
        };
        let relearning = config.relearn_steps.len() as u32;
        let computed: Result<ComputeFsrsParamsResponse> = call_typed(
            backend,
            "computeFsrsParams",
            &ComputeFsrsParamsRequest {
                search: search.clone(),
                current_params: params.clone(),
                ignore_revlogs_before_ms: 0,
                num_of_relearning_steps: relearning,
                health_check: false,
            },
        );
        match computed {
            Err(err) => optimizer_rows.push(vec![
                name.clone(),
                format!("failed: {err:#}"),
                "-".into(),
                "-".into(),
                "-".into(),
            ]),
            Ok(computed) => {
                let evaluate = |params: &[f32]| -> String {
                    let evaluated: Result<EvaluateParamsResponse> = call_typed(
                        backend,
                        "evaluateParamsLegacy",
                        &EvaluateParamsLegacyRequest {
                            params: params.to_vec(),
                            search: search.clone(),
                            ignore_revlogs_before_ms: 0,
                        },
                    );
                    match evaluated {
                        Ok(e) => format!(
                            "log loss {:.4}, RMSE {:.2}%",
                            e.log_loss,
                            e.rmse_bins * 100.0
                        ),
                        Err(err) => format!("couldn't evaluate: {err:#}"),
                    }
                };
                let same = computed.params == params || computed.params.is_empty();
                optimizer_rows.push(vec![
                    name.clone(),
                    if same {
                        "no change".to_string()
                    } else {
                        format!(
                            "{:?}",
                            computed
                                .params
                                .iter()
                                .map(|p| (p * 10000.0).round() / 10000.0)
                                .collect::<Vec<_>>()
                        )
                    },
                    computed.fsrs_items.to_string(),
                    if params.is_empty() || computed.fsrs_items == 0 {
                        "-".to_string()
                    } else {
                        evaluate(&params)
                    },
                    if same {
                        "-".to_string()
                    } else {
                        evaluate(&computed.params)
                    },
                ]);
            }
        }
    }
    if !changed.is_empty() {
        problems.push(format!(
            "these decks' settings differ from the snapshot's: {changed:?}"
        ));
    }
    out.table(
        &[
            "deck",
            "stored parameters",
            "desired ret.",
            "historical ret.",
            "new/day",
            "reviews/day",
            "max ivl",
            "ignore before",
            "param search",
        ],
        &rows,
    );
    out.line(
        "Each deck has its own, which were compared with what it used in the snapshot, with any \
         override it had for this deck alone.",
    );
    out.line("");
    out.line("### What the optimizer would suggest");
    out.line("");
    out.line(
        "Run on a throwaway copy, per deck, including its subdecks. **Nothing has been saved**; \
         that's for the owner to decide. A deck with too few reviews keeps what it has.",
    );
    out.line("");
    out.table(
        &[
            "deck",
            "suggested parameters",
            "reviews used",
            "current",
            "suggested",
        ],
        &optimizer_rows,
    );
    Ok(DeckSettingsInfo { easy_days })
}

fn due_section(
    out: &mut Out,
    backend: &Backend,
    snapshot: &Connection,
    problems: &mut Vec<String>,
) -> Result<()> {
    out.head("Due today, against a stock calculation");
    let timing: SchedTimingTodayResponse =
        call_typed(backend, "schedTimingToday", &generic::Empty {})?;
    let today = timing.days_elapsed as i64;
    let cutoff = timing.next_day_at;
    let crt = scalar(snapshot, "select crt from col")?;
    out.line(format!(
        "Day {today} of the collection, started at {crt}; the next day starts at {cutoff}. \
         \"Stock\" is SQL on the untouched snapshot with desktop Anki's rules: per deck, interday \
         learning then reviews use the review limit; new cards take what's left of it, up to the \
         new limit; intraday learning isn't limited. Limits assume nothing's been studied today."
    ));
    out.line("");
    let rows = deck_rows_today(backend)?;
    let mut table = vec![];
    let mut differ = 0;
    let mut totals = [0_i64; 6];
    for deck in rows {
        let did = deck["id"].as_i64().context("deck id")?;
        let name = deck["name"].as_str().unwrap_or_default().to_string();
        let count = |clause: String| -> Result<i64> {
            scalar(
                snapshot,
                &format!("select count() from cards where did = {did} and {clause}"),
            )
        };
        let queue = QUEUE_ONCE_OPENED_SQL;
        let new = count(format!("{queue} = 0"))?;
        let intraday = count(format!("{queue} = 1 and due < {cutoff}"))?;
        let interday = count(format!("{queue} = 3 and due <= {today}"))?;
        let review = count(format!("{queue} = 2 and due <= {today}"))?;
        let proto: Deck = call_typed(backend, "getDeck", &DeckId { did })?;
        let (new_stock, learn_stock, review_stock) = match proto.kind {
            Some(Kind::Normal(_)) => {
                let settings: DeckConfig =
                    call_typed(backend, "getDeckConfig", &DeckConfigId { dcid: did })?;
                let config = settings.config.context("deck has no settings")?;
                let review_limit = config.reviews_per_day as i64;
                let new_limit = config.new_per_day as i64;
                let capped_interday = interday.min(review_limit);
                let left = review_limit - capped_interday;
                let capped_review = review.min(left);
                let left = left - capped_review;
                (
                    new.min(new_limit).min(left),
                    intraday + capped_interday,
                    capped_review,
                )
            }
            _ => (new, intraday + interday, review),
        };
        let (new_app, learn_app, review_app) = (
            deck["new"].as_i64().unwrap_or(0),
            deck["learn"].as_i64().unwrap_or(0),
            deck["review"].as_i64().unwrap_or(0),
        );
        if (new_stock, learn_stock, review_stock) != (new_app, learn_app, review_app) {
            differ += 1;
        }
        for (slot, value) in totals.iter_mut().zip([
            new_stock,
            learn_stock,
            review_stock,
            new_app,
            learn_app,
            review_app,
        ]) {
            *slot += value;
        }
        table.push(vec![
            name,
            format!("{new_stock} / {learn_stock} / {review_stock}"),
            format!("{new_app} / {learn_app} / {review_app}"),
            format!("{new} / {} / {review}", intraday + interday),
        ]);
    }
    table.push(vec![
        "**total**".into(),
        format!("{} / {} / {}", totals[0], totals[1], totals[2]),
        format!("{} / {} / {}", totals[3], totals[4], totals[5]),
        "".into(),
    ]);
    out.table(
        &[
            "deck",
            "stock new / learn / review",
            "this app",
            "before limits",
        ],
        &table,
    );
    out.line(format!("Decks where they differ: {differ}."));
    if differ > 0 {
        problems.push(format!(
            "due counts differ from the stock calculation in {differ} decks"
        ));
    }
    Ok(())
}

fn rendering_section(
    out: &mut Out,
    backend: &Backend,
    snapshot: &Connection,
    target_media: &Path,
    problems: &mut Vec<String>,
) -> Result<()> {
    let refs = Refs::new()?;
    out.head("Rendering");
    let cards: Vec<(i64, String)> = {
        let mut statement = snapshot.prepare(
            "select c.id, n.name from cards c join notes o on o.id = c.nid \
             join notetypes n on n.id = o.mid order by c.id",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut per_type: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut failures = vec![];
    let mut blank = vec![];
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut file_users: HashMap<String, usize> = HashMap::new();
    let mut remote: BTreeSet<String> = BTreeSet::new();
    for (cid, notetype) in &cards {
        let entry = per_type.entry(notetype.clone()).or_default();
        entry[0] += 1;
        let rendered: Result<RenderCardResponse> = call_typed(
            backend,
            "renderExistingCard",
            &RenderExistingCardRequest {
                card_id: *cid,
                browser: false,
                partial_render: false,
            },
        );
        match rendered {
            Err(err) => {
                entry[1] += 1;
                failures.push(format!("{cid} ({notetype}): {err:#}"));
            }
            Ok(rendered) => {
                let question = flatten(&rendered.question_nodes);
                let answer = flatten(&rendered.answer_nodes);
                let visible = refs.tag.replace_all(&question, "");
                let has_media =
                    question.contains("[sound:") || question.to_lowercase().contains("<img");
                if rendered.is_empty || (visible.trim().is_empty() && !has_media) {
                    entry[2] += 1;
                    blank.push(*cid);
                }
                if refs.script.is_match(&question) || refs.script.is_match(&answer) {
                    entry[3] += 1;
                }
                let mut these = BTreeSet::new();
                refs.files(&question, &mut these, &mut remote);
                refs.files(&answer, &mut these, &mut remote);
                for file in these {
                    *file_users.entry(file.clone()).or_default() += 1;
                    files.insert(file);
                }
            }
        }
    }
    let rows: Vec<Vec<String>> = per_type
        .iter()
        .map(|(name, c)| {
            vec![
                name.clone(),
                c[0].to_string(),
                c[1].to_string(),
                c[2].to_string(),
                c[3].to_string(),
            ]
        })
        .collect();
    out.line(format!(
        "All {} cards rendered through `renderExistingCard`, question and answer. Errors: {}.",
        cards.len(),
        failures.len()
    ));
    out.line("");
    out.table(
        &[
            "note type",
            "cards",
            "errors",
            "blank front",
            "contain <script>, which won't run",
        ],
        &rows,
    );
    if !failures.is_empty() {
        problems.push(format!("{} cards fail to render", failures.len()));
        out.line(format!("Failures: {}", sample(&failures, 20)));
    }
    if !blank.is_empty() {
        out.line(format!("Blank fronts: {}", sample(&blank, 20)));
    }

    // templates
    out.head("Note types and templates");
    let notetypes: Vec<(i64, String, i64)> = {
        let mut statement = snapshot.prepare(
            "select n.id, n.name, (select count() from notes where mid = n.id) from notetypes n order by n.name",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut rows = vec![];
    for (ntid, name, notes) in &notetypes {
        let nt: anki_proto::notetypes::Notetype = call_typed(
            backend,
            "getNotetype",
            &anki_proto::notetypes::NotetypeId { ntid: *ntid },
        )?;
        let css = nt
            .config
            .as_ref()
            .map(|c| c.css.clone())
            .unwrap_or_default();
        let fields: BTreeSet<&str> = nt.fields.iter().map(|f| f.name.as_str()).collect();
        let mut texts = String::new();
        let mut missing = BTreeSet::new();
        for template in &nt.templates {
            let config = template.config.clone().unwrap_or_default();
            for text in [&config.q_format, &config.a_format] {
                texts.push_str(text);
                for caps in refs.field_ref.captures_iter(text) {
                    let inner = caps[1].trim();
                    let inner = inner.trim_start_matches(['#', '^', '/']);
                    let field = inner.rsplit(':').next().unwrap_or(inner).trim();
                    if !fields.contains(field)
                        && field != "FrontSide"
                        && !matches!(
                            field,
                            "Tags" | "Type" | "Deck" | "Subdeck" | "CardFlag" | "Card"
                        )
                    {
                        missing.insert(field.to_string());
                    }
                }
            }
        }
        let all = format!("{texts}\n{css}");
        let mut remote_refs: BTreeSet<String> = refs
            .remote
            .find_iter(&all)
            .map(|m| m.as_str().to_string())
            .collect();
        for caps in refs.css_url.captures_iter(&css) {
            if caps[1].contains("//") {
                remote_refs.insert(caps[1].to_string());
            } else if !caps[1].to_lowercase().starts_with("data:")
                && !target_media.join(decode_entities(&caps[1])).exists()
            {
                missing.insert(format!("css file {}", &caps[1]));
            }
        }
        rows.push(vec![
            name.clone(),
            notes.to_string(),
            nt.templates.len().to_string(),
            refs.script.find_iter(&texts).count().to_string(),
            refs.handler.find_iter(&texts).count().to_string(),
            sample(&remote_refs, 5),
            sample(&missing, 10),
            all.contains("{{type:").to_string(),
        ]);
    }
    out.table(
        &[
            "note type",
            "notes",
            "templates",
            "`<script>`",
            "inline handlers",
            "remote assets",
            "fields or css files missing",
            "type-in",
        ],
        &rows,
    );

    // field content
    let (mut with_script, mut with_handler, mut with_remote) = (vec![], vec![], vec![]);
    let mut with_remote_links = vec![];
    let remote_link = Regex::new(r#"(?i)href\s*=\s*["']?(?:https?:)?//"#)?;
    {
        let mut statement = snapshot.prepare("select id, flds from notes")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let fields: String = row.get(1)?;
            if refs.script.is_match(&fields) {
                with_script.push(id);
            }
            if refs.handler.is_match(&fields) {
                with_handler.push(id);
            }
            if refs
                .remote
                .find_iter(&fields)
                .any(|m| !remote_link.is_match(m.as_str()))
            {
                with_remote.push(id);
            }
            if remote_link.is_match(&fields) {
                with_remote_links.push(id);
            }
        }
    }
    out.line(format!(
        "Notes whose fields hold a script: {} {}; inline handlers: {} {}; anything remote that would \
         load, which the frame's CSP blocks: {} {}; plain links out, which still work: {} {}.",
        with_script.len(), sample(&with_script, 5),
        with_handler.len(), sample(&with_handler, 5),
        with_remote.len(), sample(&with_remote, 5),
        with_remote_links.len(), sample(&with_remote_links, 5),
    ));

    // file references
    out.head("Files referenced by rendered cards");
    let on_disk: BTreeSet<String> = fs::read_dir(target_media)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    let missing: Vec<&String> = files.iter().filter(|f| !on_disk.contains(*f)).collect();
    out.line(format!(
        "{} distinct files referenced by rendered cards; {} in the media folder, which has {} files; \
         {} missing; {} distinct remote images or media, which the frame's CSP won't load.",
        files.len(),
        files.len() - missing.len(),
        on_disk.len(),
        missing.len(),
        remote.len(),
    ));
    if !missing.is_empty() {
        out.line("");
        out.line(format!("Missing: {}", sample(&missing, 60)));
        problems.push(format!(
            "{} referenced media files are missing",
            missing.len()
        ));
    }
    if !remote.is_empty() {
        out.line("");
        out.line(format!("Remote: {}", sample(&remote, 30)));
    }
    out.line("");
    let mut extensions: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for file in &files {
        let entry = extensions.entry(extension(file)).or_default();
        entry.0 += 1;
        entry.1 += file_users.get(file).copied().unwrap_or(0);
    }
    let rows: Vec<Vec<String>> = extensions
        .iter()
        .map(|(ext, (distinct, uses))| {
            let risk = RISKY_FORMATS
                .iter()
                .find(|(e, _)| e == ext)
                .map_or("", |r| r.1);
            vec![
                format!("`{ext}`"),
                distinct.to_string(),
                uses.to_string(),
                risk.to_string(),
            ]
        })
        .collect();
    out.table(&["format", "files", "cards using them", "risk"], &rows);
    Ok(())
}

fn media_section(
    out: &mut Out,
    backend: &Backend,
    target: &Path,
    manifest: &Value,
    problems: &mut Vec<String>,
) -> Result<()> {
    out.head("Media folder");
    let (target_media, _) = media_paths(target);
    let copied = hash_folder(&target_media)?;
    let source = &manifest["source_hashes_before"]["media_folder"];
    out.line(format!(
        "- Copy against source: {} files, {} bytes, digest {}: {}.",
        copied["files"],
        copied["bytes"],
        copied["sha256"].as_str().unwrap_or_default(),
        if copied == *source {
            "identical"
        } else {
            "**differs**, which is expected only if media has been added since"
        },
    ));
    let source_db = super::convert::conversion_folder(target).join("source-media.db2");
    if source_db.exists() {
        let db = open_immutable(&source_db)?;
        let tracked = scalar(&db, "select count() from media")?;
        let live = scalar(&db, "select count() from media where csum is not null")?;
        let deleted = tracked - live;
        let dirty = scalar(&db, "select count() from media where dirty != 0")?;
        out.line(format!(
            "- Source's `collection.media.db2`, read from a copy: {tracked} entries, {live} live, \
             {deleted} marked deleted, {dirty} not yet synced. It tracks sync state, which this app \
             lacks, so it wasn't carried over; the app builds its own."
        ));
        let names: BTreeSet<String> = {
            let mut statement = db.prepare("select fname from media where csum is not null")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let on_disk: BTreeSet<String> = fs::read_dir(&target_media)?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        let only_db: Vec<_> = names.difference(&on_disk).collect();
        let only_disk: Vec<_> = on_disk.difference(&names).collect();
        out.line(format!(
            "- In that database but not on disk: {} {}; on disk but not in it: {} {}.",
            only_db.len(),
            sample(&only_db, 10),
            only_disk.len(),
            sample(&only_disk, 10)
        ));
    }
    let checked: CheckMediaResponse = call_typed(backend, "checkMedia", &generic::Empty {})?;
    out.line(format!(
        "- The backend's media check, on a throwaway copy, scanning fields and templates: {} \
         missing, in {} notes; {} unused. Unused files stay where they are.",
        checked.missing.len(),
        checked.missing_media_notes.len(),
        checked.unused.len()
    ));
    if !checked.missing.is_empty() {
        out.line(format!(
            "- Missing per the media check: {}",
            sample(&checked.missing, 60)
        ));
        out.line(format!(
            "- In notes: {}",
            sample(&checked.missing_media_notes, 60)
        ));
        problems.push(format!(
            "the media check says {} files are missing",
            checked.missing.len()
        ));
    }
    if !checked.unused.is_empty() {
        out.line(format!(
            "- Unused, for example: {}",
            sample(&checked.unused, 15)
        ));
    }
    Ok(())
}

fn settings_section(
    out: &mut Out,
    snapshot: &Connection,
    converted: &Connection,
    decks: &DeckSettingsInfo,
) -> Result<()> {
    out.head("Stored settings");
    let config = |db: &Connection| -> Result<BTreeMap<String, Vec<u8>>> {
        let mut statement = db.prepare("select key, val from config")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let before = config(snapshot)?;
    let after = config(converted)?;
    let text = |map: &BTreeMap<String, Vec<u8>>, key: &str| {
        map.get(key).map_or("(unset)".to_string(), |v| {
            let value = String::from_utf8_lossy(v).into_owned();
            if value.len() > 120 {
                format!("{} bytes", value.len())
            } else {
                value
            }
        })
    };
    let rows: Vec<Vec<String>> = [
        (
            "fsrs",
            "Ignored: FSRS is always on. It was already true, so opening didn't migrate anything.",
        ),
        (
            "cardStateCustomizer",
            "Ignored: scripts don't run, so nor does custom scheduling.",
        ),
        (
            "loadBalancerEnabled",
            "Used, when intervals are fuzzed on answering. There's no control for it.",
        ),
        (
            "fsrsShortTermWithStepsEnabled",
            "Used by the scheduler. There's no control for it.",
        ),
        ("rollover", "Used. The hour that a new day starts."),
        ("creationOffset", "Used, with crt, for day numbers."),
        (
            "localOffset",
            "Ignored: day boundaries use this machine's time zone.",
        ),
        (
            "schedVer",
            "Ignored apart from being read as the v3 scheduler.",
        ),
        (
            "sched2021",
            "Ignored apart from being read as the v3 scheduler.",
        ),
        ("newCardsIgnoreReviewLimit", "Used for counts."),
        ("applyAllParentLimits", "Used for counts."),
        (
            "dayLearnFirst",
            "Ignored: each deck's learning mix decides the order.",
        ),
        ("estTimes", "Ignored: answer buttons always show intervals."),
        ("dueCounts", "Ignored: the reviewer always shows counts."),
        ("hideAudioPlayButtons", "Used by the reviewer."),
        ("interruptAudioWhenAnswering", "Ignored: nothing reads it."),
        ("renderLatex", "Ignored: `[latex]` isn't rendered."),
        ("timeLim", "Ignored: no timeboxing."),
        (
            "collapseTime",
            "Used: learning cards due within this show early.",
        ),
        ("pasteImagesAsPng", "Ignored: no editor."),
        ("pasteStripsFormatting", "Ignored: no editor."),
        ("backups", "Ignored: this app has its own backup schedule."),
        ("activeDecks", "Ignored."),
        (
            "lastFsrsOptimize",
            "Ignored: the owner runs the optimizer when they choose to.",
        ),
    ]
    .iter()
    .map(|(key, note)| {
        vec![
            format!("`{key}`"),
            format!("`{}`", text(&before, key).replace('|', "\\|")),
            note.to_string(),
        ]
    })
    .collect();
    out.table(&["key", "stored value", "in this app"], &rows);
    let mut changed: Vec<String> = vec![];
    for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        if before.get(key) != after.get(key) {
            changed.push(format!(
                "{key}: {} -> {}",
                text(&before, key),
                text(&after, key)
            ));
        }
    }
    out.line(format!(
        "Config values changed since the snapshot: {}.",
        if changed.is_empty() {
            "none".to_string()
        } else {
            sample(&changed, 20)
        }
    ));
    out.line("");
    let col = |db: &Connection| -> Result<String> {
        Ok(db.query_row("select ver, crt, mod from col", [], |row| {
            Ok(format!(
                "ver {} crt {} mod {}",
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?)
    };
    out.line(format!(
        "The `col` row, before: {}; after: {}.",
        col(snapshot)?,
        col(converted)?
    ));
    out.line("");
    out.line("Easy days per deck, Monday first; empty means every day is normal. These are used as stored, with no control in the pages:");
    out.line("");
    let rows: Vec<Vec<String>> = decks
        .easy_days
        .iter()
        .map(|(name, days)| vec![name.clone(), format!("{days:?}")])
        .collect();
    out.table(&["deck", "easy days"], &rows);
    Ok(())
}

fn retrievability_section(out: &mut Out, backend: &Backend, snapshot: &Connection) -> Result<()> {
    out.head("How overdue this collection is");
    let ids: Vec<i64> = {
        let mut statement = snapshot.prepare(&format!(
            "select id from cards where {QUEUE_ONCE_OPENED_SQL} = 2"
        ))?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut values = vec![];
    for cid in ids {
        let stats: CardStatsResponse = call_typed(backend, "cardStats", &CardId { cid })?;
        if let Some(retrievability) = stats.fsrs_retrievability {
            values.push(retrievability as f64);
        }
    }
    let row = {
        let mut row = vec!["retrievability now".to_string()];
        row.extend(distribution(&mut values));
        row
    };
    out.table(
        &["", "min", "5%", "25%", "50%", "75%", "95%", "max"],
        &[row],
    );
    let newest = scalar(snapshot, "select max(id) / 1000 from revlog")?;
    out.line(format!(
        "{} review cards. The latest review in the revlog is from {}, which is why so much is due.",
        values.len(),
        chrono::DateTime::from_timestamp(newest, 0)
            .map_or("?".to_string(), |t| t.format("%Y-%m-%d").to_string())
    ));
    Ok(())
}
