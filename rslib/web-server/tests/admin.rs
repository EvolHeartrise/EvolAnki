// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Runs the real `evolanki-admin` against throwaway collections, with and
//! without a real `evolanki-server` holding them.

#![cfg(unix)]

use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use anki_proto::deck_config::deck_config::Config;
use anki_proto::decks::deck::kind_container;
use anki_proto::decks::deck::KindContainer;
use anki_proto::decks::deck::Normal;
use anki_proto::import_export::export_limit::Limit;
use anki_proto::import_export::ExportAnkiPackageOptions;
use anki_proto::import_export::ExportAnkiPackageRequest;
use anki_proto::import_export::ExportCollectionPackageRequest;
use anki_proto::import_export::ExportLimit;
use evolanki_server::collection::close_collection;
use evolanki_server::collection::open_collection;
use evolanki_server::rpc;
use evolanki_server::rpc::call_typed;
use prost::Message;
use serde_json::json;
use serde_json::Value;
use tempfile::TempDir;

fn sibling(name: &str) -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .with_file_name(name)
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    /// A closed collection with demo content: 20 Basic notes, 5 of them
    /// reviewed.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("web/sveltekit");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("index.html"), "<html></html>").unwrap();
        let fixture = Self { dir };
        drop(fixture.start(&["--create", "--seed-demo"]));
        fixture
    }

    fn collection(&self) -> PathBuf {
        self.dir.path().join("col.anki2")
    }

    fn media(&self) -> PathBuf {
        self.dir.path().join("col.media")
    }

    fn socket(&self) -> PathBuf {
        self.dir.path().join("col.admin.sock")
    }

    fn start(&self, args: &[&str]) -> Server {
        let mut child = Command::new(sibling("evolanki-server"))
            .arg("--collection")
            .arg(self.collection())
            .arg("--web-root")
            .arg(self.dir.path().join("web"))
            .args(["--port", "0", "--host", "127.0.0.1"])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let port = line.trim().rsplit(':').next().unwrap().parse().unwrap();
        Server {
            child,
            port,
            _stdout: stdout,
        }
    }

    fn raw(&self, command: &str, args: &Value) -> Value {
        let output = Command::new(sibling("evolanki-admin"))
            .arg("run")
            .arg("--collection")
            .arg(self.collection())
            .arg(command)
            .arg(args.to_string())
            .output()
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|err| panic!("{err}: {output:?}"));
        assert_eq!(
            output.status.success(),
            response["ok"] == true,
            "exit code and ok agree"
        );
        response
    }

    /// Run a command that must succeed, and return its result.
    fn ok(&self, command: &str, args: Value) -> Value {
        let response = self.raw(command, &args);
        assert_eq!(response["ok"], true, "{command} {args}: {response}");
        response["result"].clone()
    }

    fn error(&self, command: &str, args: Value) -> String {
        let response = self.raw(command, &args);
        assert_eq!(response["ok"], false, "{command} {args}: {response}");
        response["error"].as_str().unwrap().to_string()
    }

    fn backups(&self) -> usize {
        fs::read_dir(self.dir.path().join("backups"))
            .map(|dir| {
                dir.filter(|entry| {
                    let entry = entry.as_ref().unwrap();
                    entry.file_type().unwrap().is_file()
                })
                .count()
            })
            .unwrap_or(0)
    }

    fn write_file(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn add_note(&self, front: &str) -> (i64, i64) {
        let added = self.ok(
            "note.add",
            json!({"notetype":"Basic","deck":"Default","fields":{"Front":front,"Back":"b"}}),
        );
        (
            added["id"].as_i64().unwrap(),
            added["card_ids"][0].as_i64().unwrap(),
        )
    }

    fn queue_of(&self, card: i64) -> i64 {
        self.ok("card.show", json!({"id":card,"render":false}))["queue"]
            .as_i64()
            .unwrap()
    }
}

fn deck_kind(db: &rusqlite::Connection, did: i64) -> Normal {
    let kind: Vec<u8> = db
        .query_row("select kind from decks where id = ?", [did], |row| {
            row.get(0)
        })
        .unwrap();
    match KindContainer::decode(kind.as_slice()).unwrap().kind {
        Some(kind_container::Kind::Normal(normal)) => normal,
        _ => panic!("not a normal deck"),
    }
}

/// Make `did` use row 1, as an older version would have left it, with an
/// override of its own, and leave a row nothing uses.
fn make_deck_share_settings(fx: &Fixture, did: i64) {
    let db = rusqlite::Connection::open(fx.collection()).unwrap();
    let mut normal = deck_kind(&db, did);
    normal.config_id = 1;
    normal.new_limit = Some(7);
    let container = KindContainer {
        kind: Some(kind_container::Kind::Normal(normal)),
    };
    db.execute(
        "update decks set kind = ? where id = ?",
        rusqlite::params![container.encode_to_vec(), did],
    )
    .unwrap();
    db.execute("delete from deck_config where id = ?", [did])
        .unwrap();
    db.execute(
        "insert into deck_config select 12345, 'Unused', mtime_secs, config
         from deck_config where id = 1",
        [],
    )
    .unwrap();
}

fn settings_rows(path: &Path) -> Vec<i64> {
    let db = rusqlite::Connection::open(path).unwrap();
    let mut statement = db
        .prepare("select id from deck_config order by id")
        .unwrap();
    let rows = statement.query_map([], |row| row.get(0)).unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// The backup taken of the collection in the state make_deck_share_settings()
/// left it in, if there is one.
fn backup_from_before_migrating(fx: &Fixture) -> Option<PathBuf> {
    fs::read_dir(fx.dir.path().join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .find(|path| settings_rows(path).contains(&12345))
}

/// What a closed collection has if it's from before schema 19, with rows that
/// show which file a backup was taken of.
fn make_schema_18(fx: &Fixture) {
    let db = rusqlite::Connection::open(fx.collection()).unwrap();
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
            "ALTER TABLE {table} ADD COLUMN usn integer NOT NULL DEFAULT -1"
        ))
        .unwrap();
    }
    db.execute_batch(
        "CREATE INDEX ix_cards_usn ON cards (usn);
         CREATE TABLE graves (oid integer NOT NULL, type integer NOT NULL,
           usn integer NOT NULL, PRIMARY KEY (oid, type)) WITHOUT ROWID;
         INSERT INTO graves VALUES (123, 0, -1);
         ALTER TABLE col ADD COLUMN scm integer NOT NULL DEFAULT 5;
         ALTER TABLE col ADD COLUMN dty integer NOT NULL DEFAULT 0;
         ALTER TABLE col ADD COLUMN usn integer NOT NULL DEFAULT 7;
         ALTER TABLE col ADD COLUMN ls integer NOT NULL DEFAULT 8;
         UPDATE col SET ver = 18;",
    )
    .unwrap();
}

fn schema_version(path: &Path) -> u8 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("select ver from col", [], |row| row.get(0))
        .unwrap()
}

/// The backup at schema 18, with a grave still in it, if there is one.
fn backup_from_before_the_schema_change(fx: &Fixture) -> Option<PathBuf> {
    fs::read_dir(fx.dir.path().join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .find(|path| schema_version(path) == 18)
}

#[test]
fn opening_directly_backs_up_before_the_schema_change() {
    let fx = Fixture::new();
    let (note, _) = fx.add_note("kept");
    make_schema_18(&fx);
    let backups_before = fx.backups();

    fx.ok("note.show", json!({"id":note}));

    assert_eq!(schema_version(&fx.collection()), 19);
    assert_eq!(fx.backups(), backups_before + 1);
    let backup = backup_from_before_the_schema_change(&fx).unwrap();
    let graves: i64 = rusqlite::Connection::open(backup)
        .unwrap()
        .query_row("select count() from graves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(graves, 1);
}

#[test]
fn the_server_backs_up_before_the_schema_change() {
    let fx = Fixture::new();
    make_schema_18(&fx);

    let server = fx.start(&[]);
    assert!(backup_from_before_the_schema_change(&fx).is_some());
    server.stop();

    assert_eq!(schema_version(&fx.collection()), 19);
}

#[test]
fn neither_changes_the_schema_if_the_backup_fails() {
    let fx = Fixture::new();
    make_schema_18(&fx);
    fs::rename(fx.dir.path().join("backups"), fx.dir.path().join("moved")).unwrap();
    fs::write(fx.dir.path().join("backups"), "").unwrap();

    let response = fx.raw("stats.summary", &json!({}));
    assert_ne!(response["ok"], true, "{response}");
    assert_eq!(schema_version(&fx.collection()), 18);

    let status = Command::new(sibling("evolanki-server"))
        .arg("--collection")
        .arg(fx.collection())
        .arg("--web-root")
        .arg(fx.dir.path().join("web"))
        .args(["--port", "0", "--host", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(schema_version(&fx.collection()), 18);
}

#[test]
fn opening_directly_backs_up_a_collection_about_to_be_migrated() {
    let fx = Fixture::new();
    let did = fx.ok("deck.create", json!({"name":"Mine"}))["id"]
        .as_i64()
        .unwrap();
    make_deck_share_settings(&fx, did);
    let backups_before = fx.backups();

    fx.ok("stats.summary", json!({}));

    assert_eq!(fx.backups(), backups_before + 1);
    // from before it changed
    let backup = backup_from_before_migrating(&fx).unwrap();
    assert_eq!(
        deck_kind(&rusqlite::Connection::open(&backup).unwrap(), did).config_id,
        1
    );
    // and it has now
    assert_eq!(settings_rows(&fx.collection()), vec![1, did]);
    let db = rusqlite::Connection::open(fx.collection()).unwrap();
    let normal = deck_kind(&db, did);
    assert_eq!(normal.config_id, did);
    assert_eq!(normal.new_limit, None);
    let bytes: Vec<u8> = db
        .query_row(
            "select config from deck_config where id = ?",
            [did],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(Config::decode(bytes.as_slice()).unwrap().new_per_day, 7);
    drop(db);

    // nothing more to do, so no more copies from a command that only reads
    fx.ok("stats.summary", json!({}));
    assert_eq!(fx.backups(), backups_before + 1);
}

#[test]
fn a_collection_that_cant_be_backed_up_is_not_migrated() {
    let fx = Fixture::new();
    let did = fx.ok("deck.create", json!({"name":"Mine"}))["id"]
        .as_i64()
        .unwrap();
    make_deck_share_settings(&fx, did);
    // the backups folder can't be made, as this is in the way
    let folder = fx.dir.path().join("backups");
    let _ = fs::remove_dir_all(&folder);
    fs::write(&folder, b"").unwrap();

    let response = fx.raw("stats.summary", &json!({}));
    assert_ne!(response["ok"], true, "{response}");

    assert!(settings_rows(&fx.collection()).contains(&12345));
    let db = rusqlite::Connection::open(fx.collection()).unwrap();
    assert_eq!(deck_kind(&db, did).config_id, 1);
}

#[test]
fn the_server_backs_up_a_collection_about_to_be_migrated_too() {
    let fx = Fixture::new();
    let did = fx.ok("deck.create", json!({"name":"Mine"}))["id"]
        .as_i64()
        .unwrap();
    make_deck_share_settings(&fx, did);
    let backups_before = fx.backups();

    let server = fx.start(&[]);
    assert!(fx.backups() > backups_before);
    assert!(backup_from_before_migrating(&fx).is_some());
    server.stop();

    assert_eq!(settings_rows(&fx.collection()), vec![1, did]);
}

#[test]
fn a_shared_collection_is_given_its_own_settings_by_convert_and_validate_agrees() {
    let fx = Fixture::new();
    let did = fx.ok("deck.create", json!({"name":"Mine"}))["id"]
        .as_i64()
        .unwrap();
    make_deck_share_settings(&fx, did);
    fs::create_dir_all(fx.media()).unwrap();

    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("converted/col.anki2");
    let report = out.path().join("report.md");
    let run = |args: &[&std::ffi::OsStr]| -> Value {
        let output = Command::new(sibling("evolanki-admin"))
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    };
    run(&[
        "convert".as_ref(),
        "--source".as_ref(),
        fx.collection().as_ref(),
        "--target".as_ref(),
        target.as_ref(),
    ]);
    assert_eq!(settings_rows(&target), vec![1, did]);
    // the source is untouched
    assert!(settings_rows(&fx.collection()).contains(&12345));

    let validated = run(&[
        "validate".as_ref(),
        "--target".as_ref(),
        target.as_ref(),
        "--report".as_ref(),
        report.as_ref(),
        "--work-dir".as_ref(),
        out.path().as_ref(),
    ]);
    let problems = validated["result"]["problems"].as_array().unwrap();
    assert!(
        !problems
            .iter()
            .any(|p| p.as_str().unwrap().contains("settings")),
        "{problems:?}"
    );
    let report = fs::read_to_string(&report).unwrap();
    assert!(report.contains("FSRS parameters per deck"), "{report}");
    assert!(!report.to_lowercase().contains("preset"), "{report}");
    assert!(report.contains("| Mine "), "{report}");
}

#[test]
fn card_show_says_nothing_of_presets_and_preset_search_is_gone() {
    let fx = Fixture::new();
    let (_, card) = fx.add_note("q");
    let shown = fx.ok("card.show", json!({"id":card,"render":false}));
    assert!(shown.get("preset").is_none(), "{shown}");
    // not a keyword any more, so it looks in a field of that name
    let error = fx.raw("search.cards", &json!({"search":"preset:Default"}));
    let found = error["result"]["ids"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(found, 0, "{error}");
}

/// Cards can still arrive buried, in a profile from desktop Anki.
#[test]
fn buried_cards_become_normal_when_converted_or_opened() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.media()).unwrap();
    let db = rusqlite::Connection::open(fx.collection()).unwrap();
    let (sched, user): (i64, i64) = db
        .query_row(
            "select min(id), max(id) from cards where type = 0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    db.execute("update cards set queue = -2 where id = ?", [sched])
        .unwrap();
    db.execute("update cards set queue = -3 where id = ?", [user])
        .unwrap();
    drop(db);

    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("converted/col.anki2");
    let report = out.path().join("report.md");
    let run = |args: &[&std::ffi::OsStr]| -> Value {
        let output = Command::new(sibling("evolanki-admin"))
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    };
    run(&[
        "convert".as_ref(),
        "--source".as_ref(),
        fx.collection().as_ref(),
        "--target".as_ref(),
        target.as_ref(),
    ]);
    let queue_in = |path: &std::path::Path, id: i64| -> i64 {
        let db = rusqlite::Connection::open(path).unwrap();
        db.query_row("select queue from cards where id = ?", [id], |row| {
            row.get(0)
        })
        .unwrap()
    };
    assert_eq!(queue_in(&target, sched), 0);
    assert_eq!(queue_in(&target, user), 0);
    assert_eq!(
        queue_in(&fx.collection(), sched),
        -2,
        "the source is untouched"
    );

    // validate doesn't count that as cards having changed
    let validated = run(&[
        "validate".as_ref(),
        "--target".as_ref(),
        target.as_ref(),
        "--report".as_ref(),
        report.as_ref(),
        "--work-dir".as_ref(),
        out.path().as_ref(),
    ]);
    let problems = validated["result"]["problems"].as_array().unwrap();
    assert!(
        !problems
            .iter()
            .any(|p| p.as_str().unwrap().contains("differ")),
        "{problems:?}"
    );
    assert!(fs::read_to_string(&report)
        .unwrap()
        .contains("Cards in queue -2 or -3 in the source, now normal: 2"));

    // and an ordinary open does it too
    assert_eq!(fx.queue_of(sched), 0);
    assert_eq!(fx.queue_of(user), 0);
    assert_eq!(fx.ok("stats.summary", json!({}))["suspended"], 0);
}

struct Server {
    child: Child,
    port: u16,
    _stdout: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn post(&self, method: &str, body: &[u8]) -> (u16, Vec<u8>) {
        use std::io::Read;
        use std::io::Write;
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let head = format!(
            "POST /_anki/{method} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/binary\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            self.port,
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut response = vec![];
        stream.read_to_end(&mut response).unwrap();
        let split = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("no end of headers");
        let head = String::from_utf8_lossy(&response[..split]);
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, response[split + 4..].to_vec())
    }

    /// Get the next card and answer it Good, as the reviewer does.
    fn queued_card(&self) -> anki_proto::scheduler::QueuedCards {
        let (status, body) = self.post(
            "getQueuedCards",
            &anki_proto::scheduler::GetQueuedCardsRequest {
                fetch_limit: 1,
                intraday_learning_only: false,
            }
            .encode_to_vec(),
        );
        assert_eq!(status, 200);
        Message::decode(body.as_slice()).unwrap()
    }

    fn answer(&self, queued: &anki_proto::scheduler::queued_cards::QueuedCard) -> (u16, String) {
        let states = queued.states.clone().unwrap();
        let (status, body) = self.post(
            "answerCard",
            &anki_proto::scheduler::CardAnswer {
                card_id: queued.card.as_ref().unwrap().id,
                current_state: states.current,
                new_state: states.good,
                rating: anki_proto::scheduler::card_answer::Rating::Good as i32,
                answered_at_millis: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
                milliseconds_taken: 1000,
            }
            .encode_to_vec(),
        );
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    fn stop(mut self) {
        Command::new("kill")
            .arg("-TERM")
            .arg(self.child.id().to_string())
            .status()
            .unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn ids(value: &Value) -> Vec<i64> {
    value["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_i64().unwrap())
        .collect()
}

/// Every command, so it's run both directly and through a server.
fn exercise_every_command(fx: &Fixture) {
    // search and show
    assert_eq!(fx.ok("search.notes", json!({"query":""}))["count"], 20);
    assert_eq!(
        fx.ok("search.cards", json!({"query":"deck:Default","limit":2}))["ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(fx
        .error("search.cards", json!({"query":"prop:bogus"}))
        .contains("search"));

    // notes
    let (note, card) = fx.add_note("adm<b>in</b> front");
    let shown = fx.ok("note.show", json!({"id":note}));
    assert_eq!(shown["notetype"], "Basic");
    assert_eq!(shown["fields"]["Front"], "adm<b>in</b> front");
    assert_eq!(shown["cards"][0]["id"], card);
    let shown = fx.ok("card.show", json!({"id":card}));
    assert!(shown["question"].as_str().unwrap().contains("adm<b>in</b>"));
    assert_eq!(shown["deck"], "Default");
    assert!(fx
        .error(
            "note.add",
            json!({"notetype":"Basic","deck":"Default","fields":{"Front":"adm<b>in</b> front"}})
        )
        .contains("allow_duplicate"));
    assert!(fx
        .error(
            "note.add",
            json!({"notetype":"Basic","deck":"Default","fields":{"Nope":"x"}})
        )
        .contains("no field"));
    assert!(fx
        .error(
            "note.add",
            json!({"notetype":"Nope","deck":"Default","fields":{}})
        )
        .contains("no note type"));
    fx.ok(
        "note.add",
        json!({"notetype":"Basic","deck":"Default","fields":{"Front":"adm<b>in</b> front"},"allow_duplicate":true,"tags":["dup"]}),
    );
    fx.ok(
        "note.edit",
        json!({"id":note,"fields":{"Back":"edited"},"tags":["one","two"]}),
    );
    let shown = fx.ok("note.show", json!({"id":note}));
    assert_eq!(shown["fields"]["Back"], "edited");
    assert_eq!(shown["fields"]["Front"], "adm<b>in</b> front");
    assert_eq!(shown["tags"], json!(["one", "two"]));

    // media
    let file = fx.write_file("pic.png", b"first");
    let stored = fx.ok("media.add", json!({"path":file}));
    assert_eq!(stored["name"], "pic.png");
    assert_eq!(stored["renamed"], false);
    assert_eq!(fs::read(fx.media().join("pic.png")).unwrap(), b"first");
    let other = fx.write_file("pic2.png", b"second");
    let stored = fx.ok("media.add", json!({"path":other,"name":"pic.png"}));
    assert_eq!(stored["renamed"], true, "different content, so a new name");
    assert_ne!(stored["name"], "pic.png");
    let replaced = fx.ok("media.replace", json!({"path":other,"name":"pic.png"}));
    assert_eq!(fs::read(fx.media().join("pic.png")).unwrap(), b"second");
    let kept = replaced["previous_kept_at"].as_str().unwrap();
    assert_eq!(fs::read(kept).unwrap(), b"first");
    for bad in ["../evil.png", "a/b.png", "", "con.txt/"] {
        fx.error("media.replace", json!({"path":other,"name":bad}));
    }
    assert!(!fx.dir.path().join("evil.png").exists());

    // suspend, due
    assert_eq!(fx.ok("card.suspend", json!({"ids":[card]}))["changed"], 1,);
    assert_eq!(fx.queue_of(card), -1);
    assert_eq!(fx.ok("card.unsuspend", json!({"ids":[card]}))["changed"], 1);
    assert_eq!(fx.queue_of(card), 0);
    assert_eq!(
        fx.ok("card.unsuspend", json!({"ids":[card]}))["changed"],
        0,
        "it isn't suspended"
    );
    fx.ok("card.set-due", json!({"ids":[card],"days":"5"}));
    let shown = fx.ok("card.show", json!({"id":card,"render":false}));
    assert_eq!(shown["queue"], 2);
    assert!(shown["due"].as_i64().unwrap() >= 5);
    assert!(fx
        .error("card.suspend", json!({"search":""}))
        .contains("selects everything"));
    assert!(fx
        .error("card.suspend", json!({"ids":[1],"search":"x"}))
        .contains("not both"));
    assert!(fx
        .error("card.suspend", json!({"search":"deck:nothing-like-it"}))
        .contains("nothing matched"));

    // tags
    fx.ok("tag.add", json!({"ids":[note],"tags":["added::child"]}));
    assert!(fx.ok("tag.list", json!({}))["tags"]
        .as_array()
        .unwrap()
        .contains(&json!("added::child")));
    fx.ok("tag.remove", json!({"ids":[note],"tags":["one"]}));
    fx.ok("tag.rename", json!({"old":"added","new":"renamed"}));
    assert_eq!(
        fx.ok("note.show", json!({"id":note}))["tags"],
        json!(["renamed::child", "two"])
    );
    fx.error("tag.add", json!({"ids":[note],"tags":[]}));

    // decks
    let deck = fx.ok("deck.create", json!({"name":"Admin::Sub"}))["id"].clone();
    assert!(fx
        .error("deck.create", json!({"name":"Admin::Sub"}))
        .contains("already"));
    fx.ok(
        "deck.rename",
        json!({"deck":"Admin::Sub","name":"Admin::Renamed"}),
    );
    assert_eq!(
        fx.ok(
            "deck.move-cards",
            json!({"ids":[card],"deck":"Admin::Renamed"})
        )["moved"],
        1
    );
    let listed = fx.ok("deck.list", json!({}));
    let row = listed["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "Admin::Renamed")
        .unwrap()
        .clone();
    assert_eq!(row["id"], deck);
    assert_eq!(row["cards_in_deck"], 1);
    let temporary = fx.add_note("in a deck that goes").1;
    fx.ok("deck.create", json!({"name":"Doomed"}));
    fx.ok(
        "deck.move-cards",
        json!({"ids":[temporary],"deck":"Doomed"}),
    );
    assert_eq!(
        fx.ok("deck.delete", json!({"deck":"Doomed"}))["deleted_cards"],
        1
    );
    assert_eq!(
        fx.ok("search.cards", json!({"query":format!("cid:{temporary}")}))["count"],
        0
    );

    // filtered decks and custom study
    let filtered = fx.ok(
        "filtered.create",
        json!({"name":"Cram","search":"deck:Default","limit":3,"order":"added","reschedule":false}),
    );
    assert_eq!(filtered["cards"], 3);
    assert!(fx
        .error("filtered.rebuild", json!({"deck":"Default"}))
        .contains("isn't a filtered deck"));
    fx.error(
        "filtered.create",
        json!({"name":"Bad","search":"deck:Default","order":"nope"}),
    );
    assert_eq!(
        fx.ok("search.cards", json!({"query":"deck:Cram"}))["count"],
        3
    );
    fx.ok("filtered.empty", json!({"deck":"Cram"}));
    assert_eq!(
        fx.ok("search.cards", json!({"query":"deck:Cram"}))["count"],
        0
    );
    assert_eq!(
        fx.ok("filtered.rebuild", json!({"deck":"Cram"}))["cards"],
        3
    );
    assert!(fx.ok("deck.delete", json!({"deck":"Cram"}))["deleted_cards"].is_number());
    assert!(
        fx.ok("search.cards", json!({"query":"deck:Default"}))["count"]
            .as_i64()
            .unwrap()
            >= 20,
        "gave them back"
    );
    fx.ok(
        "custom-study",
        json!({"deck":"Default","new_limit_delta":5}),
    );
    fx.ok(
        "custom-study",
        json!({"deck":"Default","review_limit_delta":5}),
    );
    fx.ok(
        "custom-study",
        json!({"deck":"Default","cram":{"kind":"all","limit":2,"include":[],"exclude":[]}}),
    );
    assert_eq!(
        fx.ok(
            "search.cards",
            json!({"query":"deck:\"Custom Study Session\""})
        )["count"],
        2
    );
    fx.error("custom-study", json!({"deck":"Default"}));

    // note types
    let listed = fx.ok("notetype.list", json!({}));
    assert!(listed["notetypes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|nt| nt["name"] == "Basic" && nt["notes"].as_i64().unwrap() >= 20));
    let basic = fx.ok("notetype.show", json!({"notetype":"Basic"}));
    assert_eq!(basic["fields"], json!(["Front", "Back"]));
    assert_eq!(basic["templates"][0]["question"], "{{Front}}");
    let fields = fx.ok(
        "notetype.add-field",
        json!({"notetype":"Basic","name":"Extra"}),
    );
    assert_eq!(fields["fields"], json!(["Front", "Back", "Extra"]));
    fx.error(
        "notetype.add-field",
        json!({"notetype":"Basic","name":"Extra"}),
    );
    assert_eq!(
        fx.ok("note.show", json!({"id":note}))["fields"]["Extra"],
        "",
        "existing notes gain it"
    );
    fx.ok(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":"Card 1","question":"Q: {{Front}} {{Extra}}","css":".x{}"}),
    );
    assert!(fx.ok("card.show", json!({"id":card}))["question"]
        .as_str()
        .unwrap()
        .contains("Q: adm"));
    assert_eq!(
        fx.ok("notetype.show", json!({"notetype":"Basic"}))["css"],
        ".x{}"
    );
    fx.ok(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":0,"name":"Renamed"}),
    );
    fx.error(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":"Nope","answer":"x"}),
    );
    fx.error(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":0}),
    );
    fx.error(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":0,"question":"{{Nope}}"}),
    );

    // checks
    assert!(fx.ok("check.database", json!({}))["problems"].is_array());
    let (missing_note, _) = fx.add_note("<img src=\"not-there.png\">");
    let media = fx.ok("check.media", json!({}));
    assert!(media["missing"]
        .as_array()
        .unwrap()
        .contains(&json!("not-there.png")));
    assert!(media["missing_media_notes"]
        .as_array()
        .unwrap()
        .contains(&json!(missing_note)));
    assert!(media["unused"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().starts_with("pic")));
    fx.ok("note.delete", json!({"ids":[missing_note]}));

    // empty cards: a template that needs only Extra, and a note without it
    let two = fx.ok(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":0,"question":"{{Extra}}"}),
    );
    assert!(two.is_object());
    let empty = fx.ok("cards.empty", json!({}));
    let empties = empty["empty_cards"].as_array().unwrap().len();
    assert!(empties > 0);
    assert_eq!(empty["deleted"], 0, "only reports unless asked");
    fx.ok(
        "notetype.edit-template",
        json!({"notetype":"Basic","template":0,"question":"{{Front}}"}),
    );
    assert_eq!(fx.ok("cards.empty", json!({"apply":true}))["deleted"], 0);

    // deletion by search, and what comes of it
    let before = fx.backups();
    fx.add_note("search-delete-me");
    assert_eq!(
        fx.ok("note.delete", json!({"search":"front:search-delete-me"}))["deleted_notes"],
        1
    );
    assert!(fx.backups() > before, "a backup came first");

    // summary and backup
    let summary = fx.ok("stats.summary", json!({}));
    assert!(summary["notes"].as_i64().unwrap() >= 20);
    assert_eq!(
        summary["revlog"], 6,
        "the five, and set-due leaves an entry"
    );
    assert!(summary["with_memory_state"].as_i64().unwrap() >= 5);
    assert!(summary["due_after_limits"]["new"].is_number());
    let before = fx.backups();
    let path = fx.ok("backup.now", json!({}))["path"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(Path::new(&path).exists());
    assert_eq!(fx.backups(), before + 1);
}

#[test]
fn every_command_works_without_a_server() {
    let fx = Fixture::new();
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "direct");
    exercise_every_command(&fx);
    assert!(
        fx.error("undo", json!({})).contains("nothing to undo"),
        "undo needs a server"
    );
}

#[test]
fn every_command_works_through_a_running_server() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "server");
    exercise_every_command(&fx);
    // the server can still serve afterwards, so its connection wasn't disturbed
    let (status, _) = server.post(
        "deckTree",
        &anki_proto::decks::DeckTreeRequest { now: 1 }.encode_to_vec(),
    );
    assert_eq!(status, 200);
    server.stop();
    assert!(!fx.socket().exists(), "removed on shutdown");
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "direct");
}

#[test]
fn undo_goes_through_the_server() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    let (note, card) = fx.add_note("undo me");
    assert_eq!(fx.ok("undo.status", json!({}))["undo"], "Add Note");
    assert_eq!(fx.ok("undo", json!({}))["undid"], "Add Note");
    assert_eq!(
        fx.ok("search.notes", json!({"query":format!("nid:{note}")}))["count"],
        0
    );
    assert_eq!(
        fx.ok("search.cards", json!({"query":format!("cid:{card}")}))["count"],
        0
    );
    drop(server);
}

#[test]
fn nothing_is_reachable_over_http() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    for method in [
        "admin",
        "note.add",
        "removeNotes",
        "addMediaFile",
        "importAnkiPackage",
        "checkDatabase",
    ] {
        let (status, _) = server.post(method, b"{}");
        assert_eq!(status, 404, "{method}");
    }
    drop(server);
}

#[test]
fn socket_is_private() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    let metadata = fs::symlink_metadata(fx.socket()).unwrap();
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    // nothing is left of the private folder it was made in
    let leftovers: Vec<_> = fs::read_dir(fx.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with(".admin"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Another user can't be had here without root, but the kernel is the one who
    // checks the bits: without write permission a connection is refused.
    fs::set_permissions(fx.socket(), fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(
        UnixStream::connect(fx.socket()).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    // and that's reported, rather than treated as no server being there
    let response = fx.raw("stats.summary", &json!({}));
    assert_eq!(response["ok"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("couldn't connect"));
    fs::set_permissions(fx.socket(), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "server");
    drop(server);
}

#[test]
fn a_deep_folder_doesnt_stop_the_socket() {
    let fx = Fixture::new();
    // far more than a socket path can hold
    let deep = fx.dir.path().join("d".repeat(150));
    fs::create_dir_all(&deep).unwrap();
    let web = fx.dir.path().join("web");
    let collection = deep.join("col.anki2");
    let mut child = Command::new(sibling("evolanki-server"))
        .arg("--collection")
        .arg(&collection)
        .arg("--web-root")
        .arg(web)
        .args(["--create", "--port", "0", "--host", "127.0.0.1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.starts_with("Listening"), "{line}");
    let output = Command::new(sibling("evolanki-admin"))
        .arg("run")
        .arg("--collection")
        .arg(&collection)
        .arg("stats.summary")
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["via"], "server", "{response}");
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn stale_socket_is_cleared_and_a_live_one_isnt() {
    let fx = Fixture::new();
    let mut server = fx.start(&[]);
    // a crash leaves the socket behind, and it's not an error to anyone
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    assert!(fx.socket().exists());
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "direct");
    let server = fx.start(&[]);
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "server");

    // a second server on this collection is refused, and doesn't disturb the first
    let mut second = Command::new(sibling("evolanki-server"))
        .arg("--collection")
        .arg(fx.collection())
        .arg("--web-root")
        .arg(fx.dir.path().join("web"))
        .args(["--port", "0", "--host", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!second.wait().unwrap().success());
    assert_eq!(fx.raw("stats.summary", &json!({}))["via"], "server");
    drop(server);
}

#[test]
fn what_isnt_a_socket_is_left_alone() {
    let fx = Fixture::new();
    fs::remove_file(fx.socket()).ok();
    fs::write(fx.socket(), "mine").unwrap();
    let status = Command::new(sibling("evolanki-server"))
        .arg("--collection")
        .arg(fx.collection())
        .arg("--web-root")
        .arg(fx.dir.path().join("web"))
        .args(["--port", "0", "--host", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read_to_string(fx.socket()).unwrap(), "mine");
}

#[test]
fn a_missing_collection_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("typo.anki2");
    let output = Command::new(sibling("evolanki-admin"))
        .arg("run")
        .arg("--collection")
        .arg(&path)
        .arg("stats.summary")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!path.exists());
}

#[test]
fn bad_input_is_a_json_error() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    assert!(fx.error("no.such", json!({})).contains("unknown command"));
    assert!(fx.error("note.show", json!({})).contains("id must be"));
    let run = |args: &[&str]| {
        Command::new(sibling("evolanki-admin"))
            .arg("run")
            .arg("--collection")
            .arg(fx.collection())
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&["stats.summary", "not json"]);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(response["error"].as_str().unwrap().contains("aren't JSON"));
    drop(server);
}

#[test]
fn commands_lists_all_of_them() {
    let output = Command::new(sibling("evolanki-admin"))
        .arg("commands")
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<_> = response["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    for name in [
        "search.notes",
        "note.add",
        "media.replace",
        "import",
        "custom-study",
        "stats.summary",
    ] {
        assert!(names.contains(&name.to_string()), "{name}");
    }
}

/// A review in progress: the card is shown in a browser, and meanwhile the
/// admin tool does things. Nothing may be lost, nor anything corrupted.
#[test]
fn reviews_and_commands_can_interleave() {
    let fx = Fixture::new();
    let server = fx.start(&[]);

    // the shown card is edited: the answer still counts
    let queued = server.queued_card();
    let current = queued.cards[0].clone();
    let shown = current.card.as_ref().unwrap().clone();
    fx.ok(
        "note.edit",
        json!({"id":shown.note_id,"fields":{"Back":"edited meanwhile"}}),
    );
    assert_eq!(server.answer(&current).0, 200);
    assert_eq!(fx.ok("stats.summary", json!({}))["revlog"], 6);

    // the shown card is suspended: the answer is refused, not recorded over it
    let queued = server.queued_card();
    let current = queued.cards[0].clone();
    let id = current.card.as_ref().unwrap().id;
    fx.ok("card.suspend", json!({"ids":[id]}));
    let (status, message) = server.answer(&current);
    let revlog = fx.ok("stats.summary", json!({}))["revlog"]
        .as_i64()
        .unwrap();
    assert_ne!(status, 200, "{message}");
    assert_eq!(revlog, 6, "no review was logged");
    assert_eq!(fx.queue_of(id), -1, "still suspended");
    fx.ok("card.unsuspend", json!({"ids":[id]}));

    // the shown card is deleted
    let queued = server.queued_card();
    let current = queued.cards[0].clone();
    let shown = current.card.as_ref().unwrap().clone();
    fx.ok("note.delete", json!({"ids":[shown.note_id]}));
    assert_ne!(server.answer(&current).0, 200);
    assert_eq!(fx.ok("stats.summary", json!({}))["revlog"], 6);

    // both at once, for a while
    let total_answers = 10;
    let mut answered = 0;
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            for i in 0..25 {
                fx.add_note(&format!("alongside {i}"));
                fx.ok("deck.list", json!({}));
            }
        });
        for _ in 0..total_answers {
            let queued = server.queued_card();
            let current = queued.cards[0].clone();
            let (status, message) = server.answer(&current);
            assert_eq!(status, 200, "{message}");
            answered += 1;
        }
        reader.join().unwrap();
    });
    let summary = fx.ok("stats.summary", json!({}));
    assert_eq!(summary["revlog"], 6 + answered);
    let checked = fx.ok("check.database", json!({}));
    assert_eq!(checked["problems"], json!([]));
    server.stop();
    // and the file's fine once closed, as the tool can read what they both did
    assert_eq!(fx.ok("stats.summary", json!({}))["revlog"], 6 + answered);
}

/// Make packages with the backend, to import.
fn export_packages(fx: &Fixture) -> (PathBuf, PathBuf) {
    let backend = rpc::new_backend().unwrap();
    open_collection(&backend, &fx.collection()).unwrap();
    let apkg = fx.dir.path().join("export.apkg");
    let colpkg = fx.dir.path().join("export.colpkg");
    let _: anki_proto::generic::UInt32 = call_typed(
        &backend,
        "exportAnkiPackage",
        &ExportAnkiPackageRequest {
            out_path: apkg.to_str().unwrap().to_string(),
            options: Some(ExportAnkiPackageOptions {
                with_scheduling: true,
                with_deck_configs: true,
                with_media: true,
            }),
            limit: Some(ExportLimit {
                limit: Some(Limit::WholeCollection(Default::default())),
            }),
        },
    )
    .unwrap();
    close_collection(&backend).unwrap();
    open_collection(&backend, &fx.collection()).unwrap();
    let _: anki_proto::generic::Empty = call_typed(
        &backend,
        "exportCollectionPackage",
        &ExportCollectionPackageRequest {
            out_path: colpkg.to_str().unwrap().to_string(),
            include_media: true,
        },
    )
    .unwrap();
    (apkg, colpkg)
}

fn imports_work(start_server: bool) {
    let source = Fixture::new();
    source.ok(
        "media.add",
        json!({"path":source.write_file("in-package.png", b"packaged")}),
    );
    source.add_note("<img src=\"in-package.png\">");
    let (apkg, colpkg) = export_packages(&source);

    let fx = Fixture::new();
    let server = start_server.then(|| fx.start(&[]));
    fx.add_note("only in the target");
    let before = fx.backups();
    let imported = fx.ok("import", json!({"path":apkg,"with_scheduling":true}));
    assert!(fx.backups() > before);
    assert_eq!(imported["found_notes"], 21);
    assert_eq!(
        fs::read(fx.media().join("in-package.png")).unwrap(),
        b"packaged"
    );
    assert_eq!(imported["memory_states_added"], 0);

    assert!(fx
        .error("import", json!({"path":colpkg}))
        .contains("replace_collection"));
    assert!(fx
        .error("import", json!({"path":source.collection()}))
        .contains("only .apkg"));
    let replaced = fx.ok("import", json!({"path":colpkg,"replace_collection":true}));
    assert_eq!(replaced["replaced"], true);
    assert_eq!(
        fx.ok("search.notes", json!({"query":"only in the target"}))["count"],
        0
    );
    assert_eq!(fx.ok("search.notes", json!({"query":""}))["count"], 21);
    assert_eq!(fx.ok("stats.summary", json!({}))["revlog"], 5);
    if let Some(server) = server {
        // still serving what it now has open
        let (status, _) = server.post(
            "deckTree",
            &anki_proto::decks::DeckTreeRequest { now: 1 }.encode_to_vec(),
        );
        assert_eq!(status, 200);
        assert!(server.queued_card().cards.len() == 1);
        server.stop();
        assert!(!fx.socket().exists());
    }
    assert!(ids(&fx.ok("search.notes", json!({"query":"","limit":1}))).len() == 1);
}

#[test]
fn imports_work_directly() {
    imports_work(false);
}

#[test]
fn imports_work_through_a_server() {
    imports_work(true);
}

#[test]
fn bulk_and_destructive_commands_back_up_first_and_small_ones_dont() {
    let fx = Fixture::new();
    let server = fx.start(&[]);
    let start = fx.backups();
    let (_, card) = fx.add_note("small");
    fx.ok("card.suspend", json!({"ids":[card]}));
    assert_eq!(fx.backups(), start, "not for small changes");
    let response = fx.raw("card.unsuspend", &json!({"search":"front:small"}));
    assert_eq!(fx.backups(), start + 1, "by search");
    let reported = response["backup"].as_str().unwrap();
    assert!(Path::new(reported).exists());
    let many: Vec<i64> = (0..21).map(|_| card).collect();
    fx.ok("card.suspend", json!({"ids":many}));
    assert_eq!(fx.backups(), start + 2, "many ids");
    drop(server);
}

#[test]
fn a_failed_backup_stops_a_destructive_command() {
    let fx = Fixture::new();
    let (note, _) = fx.add_note("kept");
    // a file in place of the folder, so that backups can't work
    fs::rename(fx.dir.path().join("backups"), fx.dir.path().join("moved")).unwrap();
    fs::write(fx.dir.path().join("backups"), "").unwrap();
    assert!(fx
        .error("note.delete", json!({"ids":[note]}))
        .contains("backup failed"));
    assert_eq!(
        fx.ok("search.notes", json!({"query":format!("nid:{note}")}))["count"],
        1
    );
}

/// Fields of the notes a search finds, sorted, with their tags.
fn notes_found(fx: &Fixture, query: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let ids = fx.ok("search.notes", json!({"query":query}))["ids"].clone();
    let mut notes: Vec<(Vec<String>, Vec<String>)> = ids
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let note = fx.ok("note.show", json!({"id":id}));
            let fields = note["fields"].as_object().unwrap();
            let strings = |value: &Value| -> Vec<String> {
                value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|text| text.as_str().unwrap().to_string())
                    .collect()
            };
            (
                ["Front", "Back"]
                    .iter()
                    .map(|name| fields[*name].as_str().unwrap().to_string())
                    .collect(),
                strings(&note["tags"]),
            )
        })
        .collect();
    notes.sort();
    notes
}

fn basic_text_import(extra: Value, path: &Path) -> Value {
    let mut args = json!({
        "path": path,
        "notetype": "Basic",
        "deck": "Default",
        "delimiter": "comma",
    });
    for (key, value) in extra.as_object().unwrap() {
        args[key] = value.clone();
    }
    args
}

#[test]
fn text_import_adds_notes_and_tags_with_a_backup_first() {
    let fx = Fixture::new();
    let path = fx.write_file("words.csv", b"one,uno,first\ntwo,dos,second\n");
    let backups = fx.backups();

    let response = fx.raw(
        "import.text",
        &basic_text_import(json!({"tags":["everywhere"],"tags_column":3}), &path),
    );

    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["via"], "direct");
    assert!(response["backup"].is_string(), "{response}");
    assert!(fx.backups() > backups);
    assert_eq!(response["result"]["new"], 2);
    assert_eq!(response["result"]["found_notes"], 2);
    assert_eq!(
        notes_found(&fx, "tag:everywhere"),
        [
            (
                vec!["one".to_string(), "uno".to_string()],
                vec!["everywhere".to_string(), "first".to_string()]
            ),
            (
                vec!["two".to_string(), "dos".to_string()],
                vec!["everywhere".to_string(), "second".to_string()]
            ),
        ]
    );
}

#[test]
fn text_import_goes_through_a_running_server() {
    let fx = Fixture::new();
    let path = fx.write_file("words.csv", b"through server,yes\n");
    let server = fx.start(&[]);

    let response = fx.raw("import.text", &basic_text_import(json!({}), &path));

    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["via"], "server");
    assert_eq!(response["result"]["new"], 1);
    // and it can be undone there, as it's one operation
    assert_eq!(
        fx.ok("search.notes", json!({"query":"through"}))["count"],
        1
    );
    fx.ok("undo", json!({}));
    assert_eq!(
        fx.ok("search.notes", json!({"query":"through"}))["count"],
        0
    );
    drop(server);
}

#[test]
fn text_import_handles_duplicates_as_asked() {
    let fx = Fixture::new();
    let first = fx.write_file("first.csv", b"same,original\n");
    fx.ok(
        "import.text",
        basic_text_import(json!({"duplicates":"keep"}), &first),
    );
    let second = fx.write_file("second.csv", b"same,changed\n");

    // keep: nothing changes
    let kept = fx.ok(
        "import.text",
        basic_text_import(
            json!({"duplicates":"keep","updated_tags":["touched"]}),
            &second,
        ),
    );
    assert_eq!(kept["new"], 0);
    assert_eq!(kept["first_field_match"], 1);
    let original = vec!["same".to_string(), "original".to_string()];
    assert_eq!(notes_found(&fx, "front:same"), [(original.clone(), vec![])]);

    // update: the note changes, and gets the tags for that
    let updated = fx.ok(
        "import.text",
        basic_text_import(
            json!({"duplicates":"update","updated_tags":["touched"]}),
            &second,
        ),
    );
    assert_eq!(updated["first_field_match"], 1);
    let changed = vec!["same".to_string(), "changed".to_string()];
    assert_eq!(
        notes_found(&fx, "front:same"),
        [(changed.clone(), vec!["touched".to_string()])]
    );

    // add: a second note
    let third = fx.write_file("third.csv", b"same,another\n");
    let added = fx.ok(
        "import.text",
        basic_text_import(json!({"duplicates":"add"}), &third),
    );
    assert_eq!(added["first_field_match"], 1);
    assert_eq!(notes_found(&fx, "front:same").len(), 2);
}

#[test]
fn text_import_uses_the_file_header_and_explicit_columns() {
    let fx = Fixture::new();
    let header = fx.write_file(
        "header.txt",
        b"#separator:tab\n#notetype:Basic\n#deck:From header\n#tags:head\nq\ta\n",
    );
    let imported = fx.ok("import.text", json!({"path":header}));
    assert_eq!(imported["new"], 1);
    assert_eq!(
        fx.ok(
            "search.notes",
            json!({"query":"deck:\"From header\" tag:head"})
        )["count"],
        1
    );

    let swapped = fx.write_file("swapped.csv", b"back text,front text\n");
    fx.ok(
        "import.text",
        basic_text_import(json!({"fields":{"Front":2,"Back":1}}), &swapped),
    );
    assert_eq!(
        notes_found(&fx, "front:\"front text\""),
        [(
            vec!["front text".to_string(), "back text".to_string()],
            vec![]
        )]
    );
}

#[test]
fn text_import_refuses_what_it_cant_do() {
    let fx = Fixture::new();
    let path = fx.write_file("words.csv", b"a,b\n");
    assert!(fx
        .error(
            "import.text",
            json!({"path":fx.dir.path().join("missing.csv")})
        )
        .contains("isn't a file"));
    assert!(fx
        .error(
            "import.text",
            basic_text_import(json!({"duplicates":"sometimes"}), &path)
        )
        .contains("duplicates"));
    assert!(fx
        .error(
            "import.text",
            basic_text_import(json!({"fields":{"Nope":1}}), &path)
        )
        .contains("no field"));
    assert!(fx
        .error("import.text", json!({"path":path,"fields":{"Front":1}}))
        .contains("needs notetype"));
    // and an empty file is an error, not nothing
    let empty = fx.write_file("empty.csv", b"");
    let response = fx.raw("import.text", &json!({"path":empty}));
    assert_ne!(response["ok"], true, "{response}");
}
