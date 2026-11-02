// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Cards the real data doesn't have but an import could bring in: no memory
//! state, with ease factors and SM-2 history.
//!
//! The fixtures are written once and reused, so a later build reads the very
//! same file, including columns it may no longer write itself.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;

use crate::clock;
use crate::harness::prepare_output;
use crate::harness::Harness;
use crate::harness::RATINGS;
use crate::json::write_output;

const DAY_MS: i64 = 86_400_000;
const FIXTURE_AGE_DAYS: i64 = 400;

/// Name, type, queue, due relative to today, interval, factor, reps, lapses,
/// left, history rows.
type Case = (
    &'static str,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    Option<&'static str>,
);

const CASES: &[Case] = &[
    (
        "review_factor_2500_no_history",
        2,
        2,
        0,
        10,
        2500,
        5,
        0,
        0,
        None,
    ),
    (
        "review_factor_1300_overdue",
        2,
        2,
        -30,
        100,
        1300,
        20,
        5,
        0,
        None,
    ),
    ("review_factor_0", 2, 2, 0, 5, 0, 5, 0, 0, None),
    (
        "review_factor_5000_long",
        2,
        2,
        0,
        1000,
        5000,
        9,
        0,
        0,
        None,
    ),
    ("review_factor_1100", 2, 2, 0, 12, 1100, 9, 0, 0, None),
    ("review_interval_0", 2, 2, 0, 0, 2500, 1, 0, 0, None),
    ("review_due_early", 2, 2, 20, 30, 2500, 5, 0, 0, None),
    (
        "review_sm2_history",
        2,
        2,
        0,
        16,
        2360,
        5,
        1,
        0,
        Some("sm2"),
    ),
    (
        "review_manual_reschedule_history",
        2,
        2,
        0,
        16,
        2500,
        3,
        0,
        0,
        Some("manual"),
    ),
    (
        "review_history_only_learning",
        2,
        2,
        0,
        4,
        2500,
        2,
        0,
        0,
        Some("learn"),
    ),
    ("review_suspended", 2, -1, 0, 20, 2500, 5, 0, 0, Some("sm2")),
    (
        "learning_with_history",
        1,
        1,
        0,
        0,
        0,
        1,
        0,
        1001,
        Some("learn"),
    ),
    ("learning_no_history", 1, 1, 0, 0, 0, 0, 0, 2002, None),
    ("day_learning", 1, 3, 0, 0, 0, 1, 0, 1001, Some("learn")),
    (
        "relearning_factor_1800",
        3,
        1,
        0,
        30,
        1800,
        12,
        3,
        1001,
        Some("sm2"),
    ),
    (
        "day_relearning",
        3,
        3,
        0,
        30,
        2100,
        12,
        3,
        1001,
        Some("sm2"),
    ),
    ("new", 0, 0, 1, 0, 0, 0, 0, 0, None),
    ("new_with_old_factor", 0, 0, 2, 0, 2500, 0, 0, 0, None),
];

fn build_fixtures(fixtures: &Path, now: i64) -> Result<()> {
    fs::create_dir_all(fixtures)?;
    let collection = fixtures.join("synthetic.anki2");
    let work = fixtures.join("work");
    let mut harness = Harness::new(&work)?;
    let created = now * 1000 - FIXTURE_AGE_DAYS * DAY_MS;
    clock::set_ms(created);
    harness.open(&collection)?;
    let notetypes = harness.must("GetNotetypeNames", json!({}))?;
    let notetype = notetypes["entries"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["name"] == "Basic")
                .cloned()
        })
        .ok_or_else(|| anyhow::anyhow!("no Basic notetype"))?;
    for case in CASES {
        clock::advance_ms(1000);
        let mut note = harness.must("NewNote", json!({ "ntid": notetype["id"] }))?;
        note["fields"] = json!([case.0, "back"]);
        harness.must("AddNote", json!({ "note": note, "deck_id": 1 }))?;
    }
    clock::set_ms(now * 1000);
    let today = harness.timing()?.today;
    harness.close()?;

    let db = rusqlite::Connection::open(&collection)?;
    let has_factor = db
        .prepare("select 1 from pragma_table_info('cards') where name = 'factor'")?
        .exists([])?;
    let mut cids = vec![];
    {
        let mut statement = db.prepare("select id from cards order by id")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            cids.push(row.get::<_, i64>(0)?);
        }
    }
    for (cid, case) in cids.iter().zip(CASES) {
        let &(_, ctype, queue, due, interval, factor, reps, lapses, left, history) = case;
        // learning cards in the learning queue are due at a timestamp
        let due = if queue == 1 { now - 60 } else { today + due };
        db.execute(
            "update cards set type=?, queue=?, due=?, ivl=?, reps=?, lapses=?, left=?, data=''
             where id=?",
            rusqlite::params![ctype, queue, due, interval, reps, lapses, left, cid],
        )?;
        if has_factor {
            db.execute(
                "update cards set factor=? where id=?",
                rusqlite::params![factor, cid],
            )?;
        }
        // type, button, interval, last interval, factor
        let rows: &[(i64, i64, i64, i64, i64)] = match history {
            Some("sm2") => &[
                (0, 3, -60, -60, 0),
                (0, 3, 1, -60, 2500),
                (1, 3, 3, 1, 2500),
                (1, 2, 4, 3, 2350),
                (1, 1, -600, 4, 2150),
                (2, 3, 2, -600, 2150),
                (1, 3, 5, 2, 2150),
                (1, 4, 16, 5, 2300),
            ],
            Some("manual") => &[(0, 3, 1, -60, 2500), (4, 0, 7, 1, 0), (1, 3, 16, 7, 2500)],
            Some("learn") => &[(0, 3, -600, -60, 0)],
            _ => &[],
        };
        for (idx, row) in rows.iter().enumerate() {
            let age = rows.len() as i64 - idx as i64;
            db.execute(
                "insert into revlog (id,cid,usn,ease,ivl,lastIvl,factor,time,type)
                 values (?,?,-1,?,?,?,?,5000,?)",
                rusqlite::params![
                    now * 1000 - age * 12 * DAY_MS + (*cid / 1000 % 100_000),
                    cid,
                    row.1,
                    row.2,
                    row.3,
                    row.4,
                    row.0
                ],
            )?;
        }
    }
    db.execute(
        "update config set val = cast('true' as blob) where key = 'fsrs'",
        [],
    )?;
    drop(db);

    clock::set_ms(now * 1000);
    harness.open(&collection)?;
    harness.must(
        "ExportAnkiPackage",
        json!({
            "out_path": fixtures.join("synthetic.apkg").to_str(),
            "options": {
                "with_scheduling": true,
                "with_deck_configs": true,
                "with_media": false,
            },
            "limit": { "whole_collection": {} },
        }),
    )?;
    harness.close()?;
    fs::remove_dir_all(work)?;
    let media = collection.with_extension("media");
    let _ = fs::remove_dir_all(media);
    let _ = fs::remove_file(collection.with_extension("mdb"));
    Ok(())
}

fn label(harness: &mut Harness, cid: i64) -> String {
    let card = harness.card(cid);
    let note = harness.record("GetNote", json!({ "nid": card["note_id"] }));
    note["fields"][0].as_str().unwrap_or("").to_string()
}

fn capture(harness: &mut Harness) -> Result<Value> {
    let mut out = Map::new();
    let mut per_card = Map::new();
    let cids = harness.search("")?;
    for cid in &cids {
        per_card.insert(
            label(harness, *cid),
            json!({
                "card": harness.card(*cid),
                "next": harness.card_states(*cid),
                "stats": harness.record("CardStats", json!({ "cid": cid })),
            }),
        );
    }
    out.insert("cards".into(), Value::Object(per_card));
    out.insert("queue_counts".into(), {
        let queued = harness.record(
            "GetQueuedCards",
            json!({ "fetch_limit": 1, "intraday_learning_only": false }),
        );
        json!([
            queued["new_count"],
            queued["learning_count"],
            queued["review_count"],
            queued["error"]
        ])
    });
    out.insert(
        "graphs".into(),
        harness.record("Graphs", json!({ "search": "", "days": 365 })),
    );
    Ok(Value::Object(out))
}

/// Answer each card once, without the queue being involved, and again to see
/// the state the first answer left behind.
fn answer_all(harness: &mut Harness, rating: usize) -> Result<Value> {
    let mut out = Map::new();
    let cids = harness.search("")?;
    for cid in &cids {
        let name = label(harness, *cid);
        let states = harness.rpc("GetSchedulingStates", json!({ "cid": cid }));
        let entry = match states {
            Err(err) => json!({ "states_error": err }),
            Ok(states) => {
                let result = harness.answer(*cid, &states, rating, 4000);
                json!({
                    "error": result.err(),
                    "after": harness.card(*cid),
                    "revlog": harness.latest_revlog(*cid),
                    "then": harness.card_states(*cid),
                })
            }
        };
        out.insert(name, entry);
        clock::advance_ms(1000);
    }
    Ok(Value::Object(out))
}

enum Start<'a> {
    Collection {
        fsrs_flag: bool,
    },
    Package {
        file: &'a str,
        with_scheduling: bool,
    },
}

fn open_scenario(harness: &mut Harness, fixtures: &Path, start: &Start) -> Result<Value> {
    let collection = fixtures.join("synthetic.anki2");
    let mut import_result = Value::Null;
    match start {
        Start::Collection { fsrs_flag } => {
            let target = fixtures.join("work").join("scenario-copy.anki2");
            fs::copy(&collection, &target)?;
            {
                let db = rusqlite::Connection::open(&target)?;
                db.execute(
                    "update config set val = cast(? as blob) where key = 'fsrs'",
                    [fsrs_flag.to_string()],
                )?;
            }
            harness.open_copy(&target, "scenario")?;
            fs::remove_file(target)?;
        }
        Start::Package {
            file,
            with_scheduling,
        } => {
            let target = fixtures.join("work").join("empty.anki2");
            harness.open(&target)?;
            harness.close()?;
            harness.open_copy(&target, "scenario")?;
            for extra in ["empty.anki2", "empty.mdb"] {
                let _ = fs::remove_file(fixtures.join("work").join(extra));
            }
            let _ = fs::remove_dir_all(fixtures.join("work").join("empty.media"));
            import_result = harness.record(
                "ImportAnkiPackage",
                json!({
                    "package_path": fixtures.join(file).to_str(),
                    "options": {
                        "merge_notetypes": false,
                        "with_scheduling": with_scheduling,
                        "with_deck_configs": true,
                    },
                }),
            );
            import_result = json!({
                "error": import_result["error"],
                "new": import_result["log"]["new"].as_array().map(Vec::len),
            });
        }
    }
    Ok(import_result)
}

pub fn run(fixtures: &Path, out: &Path, now: i64) -> Result<()> {
    let out = prepare_output(out)?;
    let fixtures = prepare_output(fixtures)?;
    // written last, so a failed build isn't mistaken for a fixture
    if !fixtures.join("synthetic.apkg").exists() {
        let _ = fs::remove_file(fixtures.join("synthetic.anki2"));
        eprintln!("building fixtures");
        build_fixtures(&fixtures, now)?;
    }
    if !fixtures.join("synthetic-legacy.apkg").exists() {
        // this version can't write a package in the old format, so it can't make this
        anyhow::bail!("{} is missing a kept fixture", fixtures.display());
    }
    let work = fixtures.join("work");
    let _ = fs::remove_dir_all(&work);
    let mut harness = Harness::new(&work)?;
    let scenarios: [(&str, Start); 5] = [
        (
            "collection_flag_on_no_migration",
            Start::Collection { fsrs_flag: true },
        ),
        (
            "collection_flag_off_migrates",
            Start::Collection { fsrs_flag: false },
        ),
        (
            "apkg_with_scheduling",
            Start::Package {
                file: "synthetic.apkg",
                with_scheduling: true,
            },
        ),
        (
            "apkg_without_scheduling",
            Start::Package {
                file: "synthetic.apkg",
                with_scheduling: false,
            },
        ),
        (
            "legacy_apkg_with_scheduling",
            Start::Package {
                file: "synthetic-legacy.apkg",
                with_scheduling: true,
            },
        ),
    ];
    let mut output = Map::new();
    for (name, start) in &scenarios {
        eprintln!("scenario: {name}");
        clock::set_ms(now * 1000);
        let mut scenario = Map::new();
        let import = open_scenario(&mut harness, &fixtures, start)?;
        scenario.insert("import".into(), import);
        scenario.insert("initial".into(), capture(&mut harness)?);
        harness.close()?;
        for (rating, (rating_name, _)) in RATINGS.iter().enumerate() {
            clock::set_ms(now * 1000);
            let _ = open_scenario(&mut harness, &fixtures, start)?;
            scenario.insert(
                format!("answer_{rating_name}"),
                answer_all(&mut harness, rating)?,
            );
            harness.close()?;
        }
        output.insert((*name).to_string(), Value::Object(scenario));
    }
    output.insert(
        "meta".into(),
        json!({ "now": now, "request_fields_no_longer_present": harness.skipped }),
    );
    write_output(&out.join("synthetic.json"), &output)?;
    fs::remove_dir_all(&work)?;
    Ok(())
}
