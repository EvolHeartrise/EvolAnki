// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The output for a real collection. Each section works on its own copy.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;

use crate::clock;
use crate::harness::check_input_path;
use crate::harness::flat_state;
use crate::harness::flat_states;
use crate::harness::prepare_output;
use crate::harness::raw_rows;
use crate::harness::Harness;
use crate::harness::StateContext;
use crate::harness::RATINGS;
use crate::json::splitmix;
use crate::json::write_output;
use crate::json::Rng;

pub struct Config {
    pub collection: PathBuf,
    pub out: PathBuf,
    pub now: i64,
    pub seed: u64,
    pub answers: usize,
    pub days: usize,
    pub only: BTreeSet<String>,
}

pub fn run(config: &Config) -> Result<()> {
    let source = check_input_path(&config.collection)?;
    let out = prepare_output(&config.out)?;
    let work = out.join("work");
    let _ = fs::remove_dir_all(&work);
    let mut harness = Harness::new(&work)?;
    type Section = fn(&mut Harness, &Path, &Config) -> Result<Map<String, Value>>;
    let sections: [(&str, Section); 4] = [
        ("snapshot", snapshot),
        ("replay", replay),
        ("ops", ops),
        ("tools", tools),
    ];
    for (name, section) in sections {
        if !config.only.contains(name) {
            continue;
        }
        eprintln!("section: {name}");
        clock::set_ms(config.now * 1000);
        let mut output = section(&mut harness, &source, config)?;
        output.insert(
            "meta".into(),
            json!({
                "now": config.now,
                "seed": config.seed,
                "request_fields_no_longer_present": harness.skipped,
            }),
        );
        write_output(&out.join(format!("{name}.json")), &output)?;
    }
    fs::remove_dir_all(&work)?;
    Ok(())
}

fn decks(harness: &mut Harness) -> Result<Vec<(i64, String)>> {
    let response = harness.must("GetDeckNames", json!({ "include_filtered": true }))?;
    Ok(response["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            (
                entry["id"].as_i64().unwrap_or(0),
                entry["name"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect())
}

fn deck_search(name: &str) -> String {
    format!("deck:\"{}\"", name.replace('"', "\\\""))
}

/// The deck with the most cards of its own, and the lowest name on a tie.
fn biggest_deck(harness: &mut Harness) -> Result<(i64, String, Vec<i64>)> {
    let mut best = (0, String::new(), vec![]);
    for (id, name) in decks(harness)? {
        let cards = harness.search(&deck_search(&name))?;
        if cards.len() > best.2.len() {
            best = (id, name, cards);
        }
    }
    Ok(best)
}

fn stats(harness: &mut Harness) -> Result<Value> {
    let mut out = Map::new();
    out.insert(
        "studied_today".into(),
        harness.record("StudiedToday", json!({})),
    );
    out.insert(
        "graphs_all_days".into(),
        harness.record("Graphs", json!({ "search": "", "days": 0 })),
    );
    out.insert(
        "graphs_year".into(),
        harness.record("Graphs", json!({ "search": "", "days": 365 })),
    );
    for (id, name) in decks(harness)? {
        out.insert(
            format!("deck {id}"),
            json!({
                "graphs_year": harness.record(
                    "Graphs",
                    json!({ "search": deck_search(&name), "days": 365 }),
                ),
                "today": harness.record("CountsForDeckToday", json!({ "did": id })),
            }),
        );
    }
    Ok(Value::Object(out))
}

fn queues(harness: &mut Harness) -> Result<Value> {
    let mut out = Map::new();
    for balancer in [true, false] {
        harness.must("SetLoadBalancerEnabled", json!({ "val": balancer }))?;
        let mut by_deck = Map::new();
        for (id, _) in decks(harness)? {
            harness.must("SetCurrentDeck", json!({ "did": id }))?;
            let timing = harness.timing()?;
            let queued = harness.record(
                "GetQueuedCards",
                json!({ "fetch_limit": 1_000_000, "intraday_learning_only": false }),
            );
            let cards: Vec<Value> = queued["cards"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|item| {
                    let context = StateContext {
                        timing,
                        desired_retention: item["context"]["desired_retention"]
                            .as_f64()
                            .unwrap_or(0.0) as f32,
                        decay: item["context"]["decay"].as_f64().unwrap_or(0.0) as f32,
                    };
                    json!({
                        "id": item["card"]["id"],
                        "queue": item["queue"],
                        "states": flat_states(&item["states"], &context),
                        "context": item["context"],
                    })
                })
                .collect();
            by_deck.insert(
                id.to_string(),
                json!({
                    "new_count": queued["new_count"],
                    "learning_count": queued["learning_count"],
                    "review_count": queued["review_count"],
                    "error": queued["error"],
                    "cards": cards,
                }),
            );
        }
        out.insert(format!("load_balancer_{balancer}"), Value::Object(by_deck));
    }
    Ok(Value::Object(out))
}

fn snapshot(harness: &mut Harness, source: &Path, _: &Config) -> Result<Map<String, Value>> {
    let path = harness.open_copy(source, "snapshot")?;
    let timing = harness.timing()?;
    let mut output = Map::new();
    let cids = harness.search("")?;
    eprintln!("  {} cards", cids.len());
    // Before any queue exists, so these don't involve the load balancer.
    let mut per_card = Map::new();
    for cid in &cids {
        let mut item = Map::new();
        item.insert("card".into(), harness.card(*cid));
        item.insert("next".into(), harness.card_states(*cid));
        item.insert(
            "stats".into(),
            harness.record("CardStats", json!({ "cid": cid })),
        );
        item.insert(
            "fuzz_delta".into(),
            Value::Array(
                [3, 7, 30, 100, 1000]
                    .iter()
                    .map(|interval| {
                        harness.record("FuzzDelta", json!({ "card_id": cid, "interval": interval }))
                            ["delta_days"]
                            .clone()
                    })
                    .collect(),
            ),
        );
        per_card.insert(cid.to_string(), Value::Object(item));
    }
    output.insert("cards".into(), Value::Object(per_card));
    output.insert(
        "timing".into(),
        json!({ "today": timing.today, "next_day_at": timing.next_day_at }),
    );
    output.insert(
        "deck_tree".into(),
        harness.record("DeckTree", json!({ "now": clock::now_secs() })),
    );
    output.insert("stats".into(), stats(harness)?);
    output.insert("queues".into(), queues(harness)?);
    harness.close()?;
    fs::remove_file(path)?;
    Ok(output)
}

/// Answer the next card in the queue, if there is one.
fn answer_next(
    harness: &mut Harness,
    rng: &mut Rng,
    coverage: &mut BTreeMap<String, u64>,
) -> Result<Option<Value>> {
    let queued = harness.must(
        "GetQueuedCards",
        json!({ "fetch_limit": 1, "intraday_learning_only": false }),
    )?;
    let Some(item) = queued["cards"].get(0).cloned() else {
        return Ok(None);
    };
    let cid = item["card"]["id"].as_i64().unwrap_or(0);
    let roll = rng.below(100);
    let rating = match roll {
        0..=14 => 0,
        15..=29 => 1,
        30..=84 => 2,
        _ => 3,
    };
    let millis = 1000 + rng.below(20_000);
    let timing = harness.timing()?;
    let context = StateContext {
        timing,
        desired_retention: item["context"]["desired_retention"].as_f64().unwrap_or(0.0) as f32,
        decay: item["context"]["decay"].as_f64().unwrap_or(0.0) as f32,
    };
    let (name, key) = RATINGS[rating];
    // what's stored, as the queue's copy has its FSRS fields filled in
    let stored = harness.card(cid);
    let result = harness.answer(cid, &item["states"], rating, millis);
    *coverage
        .entry(format!(
            "type {} queue {} {name}",
            item["card"]["ctype"].as_i64().unwrap_or(0),
            item["card"]["queue"].as_i64().unwrap_or(0),
        ))
        .or_default() += 1;
    Ok(Some(json!({
        "op": "answer",
        "now": clock::now_ms(),
        "card_id": cid,
        "before": stored,
        "queued_card": item["card"],
        "queue": item["queue"],
        "counts": [queued["new_count"], queued["learning_count"], queued["review_count"]],
        "rating": name,
        "chosen": flat_state(&item["states"][key], &context),
        "error": result.err(),
        "after": harness.card(cid),
        "revlog": harness.latest_revlog(cid),
    })))
}

fn replay(harness: &mut Harness, source: &Path, config: &Config) -> Result<Map<String, Value>> {
    let path = harness.open_copy(source, "replay")?;
    let mut rng = Rng::new(config.seed);
    let start_ms = clock::now_ms();
    let per_day = (config.answers / config.days.max(1)).max(1);
    let mut events = vec![];
    let mut coverage = BTreeMap::new();
    let (mut answers, mut undos, mut redos, mut undo_mismatches) = (0usize, 0, 0, 0);
    let all_decks: Vec<i64> = decks(harness)?.iter().map(|deck| deck.0).collect();
    let mut day = 0;
    while day < config.days * 8 && (answers < config.answers || day < config.days) {
        let deck = all_decks[day % all_decks.len()];
        for id in &all_decks {
            harness.record(
                "ExtendLimits",
                json!({ "deck_id": id, "new_delta": 60, "review_delta": 300 }),
            );
        }
        harness.must("SetCurrentDeck", json!({ "did": deck }))?;
        events.push(json!({
            "op": "day",
            "day": day,
            "now": clock::now_ms(),
            "deck": deck,
            "tree": harness.record("DeckTree", json!({ "now": clock::now_secs() }))["review_count"],
        }));
        for _ in 0..per_day {
            // a card suspended while it's on screen, then brought back
            if rng.chance(5) {
                let cards = harness.search("is:suspended")?;
                if let Some(cid) = cards.get(rng.below(cards.len() as u64) as usize).copied() {
                    let states = harness.must("GetSchedulingStates", json!({ "cid": cid }))?;
                    let refused = harness.answer(cid, &states, 2, 1000);
                    harness.record("UnsuspendCards", json!({ "cids": [cid] }));
                    let unsuspended = harness.card(cid);
                    harness.record("SuspendCards", json!({ "card_ids": [cid] }));
                    events.push(json!({
                        "op": "answer_suspended",
                        "card_id": cid,
                        "result": refused.err(),
                        "unsuspended": unsuspended,
                        "after": harness.card(cid),
                    }));
                    *coverage
                        .entry("suspended, refused".to_string())
                        .or_default() += 1u64;
                }
            }
            let Some(event) = answer_next(harness, &mut rng, &mut coverage)? else {
                break;
            };
            let cid = event["card_id"].as_i64().unwrap_or(0);
            let before = event["before"].clone();
            let after = event["after"].clone();
            answers += 1;
            events.push(event);
            clock::advance_ms(5000 + rng.below(40_000) as i64);
            if rng.chance(60) {
                undos += 1;
                let result = harness.record("Undo", json!({}));
                let restored = harness.card(cid);
                undo_mismatches += u32::from(restored != before);
                events.push(json!({
                    "op": "undo",
                    "card_id": cid,
                    "operation": result["operation"],
                    "error": result["error"],
                    "restored_exactly": restored == before,
                    "card": restored,
                    "revlog": harness.latest_revlog(cid),
                }));
                if rng.chance(500) {
                    redos += 1;
                    let result = harness.record("Redo", json!({}));
                    let redone = harness.card(cid);
                    events.push(json!({
                        "op": "redo",
                        "card_id": cid,
                        "operation": result["operation"],
                        "error": result["error"],
                        "reapplied_exactly": redone == after,
                        "card": redone,
                        "revlog": harness.latest_revlog(cid),
                    }));
                }
            }
        }
        let next_day_at = harness.timing()?.next_day_at;
        // Each fourth session starts shortly before the day ends, so learning
        // steps cross the rollover and become day learning.
        let time_of_day = if (day + 1) % 4 == 0 {
            86_400 - 900
        } else {
            3600 + rng.below(36_000) as i64
        };
        clock::set_ms((next_day_at + time_of_day) * 1000);
        day += 1;
    }
    let timing = harness.timing()?;
    let mut output = Map::new();
    output.insert(
        "summary".into(),
        json!({
            "answers": answers,
            "undos": undos,
            "undo_mismatches": undo_mismatches,
            "redos": redos,
            "days": day,
            "today": timing.today,
            "coverage": coverage,
        }),
    );
    eprintln!("  {}", output["summary"]);
    output.insert("events".into(), Value::Array(events));
    output.insert("stats_after".into(), stats(harness)?);
    harness.close()?;
    output.insert("final_cards".into(), raw_rows(&path, "cards", "")?);
    output.insert(
        "final_new_revlog".into(),
        raw_rows(&path, "revlog", &format!("where id >= {start_ms}"))?,
    );
    fs::remove_file(path)?;
    Ok(output)
}

/// A fixed sample, chosen by hashing, so it doesn't follow id order.
fn sample(cids: &[i64], count: usize, salt: u64) -> Vec<i64> {
    let mut hashed: Vec<(u64, i64)> = cids
        .iter()
        .map(|cid| (splitmix(*cid as u64 ^ salt), *cid))
        .collect();
    hashed.sort_unstable();
    let mut out: Vec<i64> = hashed.iter().take(count).map(|item| item.1).collect();
    out.sort_unstable();
    out
}

fn filtered_case(
    harness: &mut Harness,
    seed: u64,
    name: &str,
    reschedule: bool,
    order: &str,
) -> Result<Value> {
    let source_search = "-is:suspended (is:new or is:learn or is:review)";
    let mut request = harness.must("GetOrCreateFilteredDeck", json!({ "did": 0 }))?;
    request["name"] = name.into();
    request["allow_empty"] = true.into();
    request["config"]["reschedule"] = reschedule.into();
    request["config"]["search_terms"] =
        json!([{ "search": source_search, "limit": 40, "order": order }]);
    request["config"]["preview_again_secs"] = 60.into();
    request["config"]["preview_hard_secs"] = 600.into();
    request["config"]["preview_good_secs"] = 0.into();
    let mut rng = Rng::new(seed);
    let built = harness.record("AddOrUpdateFilteredDeck", request);
    let Some(did) = built["id"].as_i64() else {
        return Ok(json!({ "error": built["error"] }));
    };
    let members = harness.search(&deck_search(name))?;
    let in_deck = harness.cards(&members);
    let states: Map<String, Value> = members
        .iter()
        .map(|cid| (cid.to_string(), harness.card_states(*cid)))
        .collect();
    harness.must("SetCurrentDeck", json!({ "did": did }))?;
    let mut coverage = BTreeMap::new();
    let mut answered = vec![];
    for _ in 0..25 {
        match answer_next(harness, &mut rng, &mut coverage)? {
            Some(event) => answered.push(event),
            None => break,
        }
        clock::advance_ms(10_000);
    }
    let rebuilt = harness.record("RebuildFilteredDeck", json!({ "did": did }));
    let after_rebuild = harness.search(&deck_search(name))?;
    let emptied = harness.record("EmptyFilteredDeck", json!({ "did": did }));
    Ok(json!({
        "members": in_deck,
        "states": states,
        "answered": answered,
        "rebuild": rebuilt["count"],
        "after_rebuild": after_rebuild,
        "empty_error": emptied["error"],
        "after_empty": harness.cards(&members),
    }))
}

fn change_case(harness: &mut Harness, method: &str, input: Value, cids: &[i64]) -> Value {
    harness.reseed(&format!("{method} {input}"));
    let before = harness.cards(cids);
    let result = harness.record(method, input);
    let after = harness.cards(cids);
    let states: Map<String, Value> = cids
        .iter()
        .map(|cid| (cid.to_string(), harness.card_states(*cid)))
        .collect();
    let revlog: Map<String, Value> = cids
        .iter()
        .map(|cid| (cid.to_string(), harness.latest_revlog(*cid)))
        .collect();
    let undone = harness.record("Undo", json!({}));
    let restored = harness.cards(cids);
    json!({
        "error": result["error"],
        "after": after,
        "states": states,
        "revlog": revlog,
        "undo_error": undone["error"],
        "undo_restored_exactly": restored == before,
    })
}

fn ops(harness: &mut Harness, source: &Path, config: &Config) -> Result<Map<String, Value>> {
    let mut output = Map::new();
    let orders = [
        "OLDEST_REVIEWED_FIRST",
        "RANDOM",
        "INTERVALS_ASCENDING",
        "INTERVALS_DESCENDING",
        "LAPSES",
        "ADDED",
        "DUE",
        "REVERSE_ADDED",
        "RETRIEVABILITY_ASCENDING",
        "RETRIEVABILITY_DESCENDING",
        "RELATIVE_OVERDUENESS",
    ];
    // Each case gets its own copy, clock and generator, so that if a case's
    // `RANDOM` picks ever moved, nothing else would follow them.
    // A case gets a fixed span of time: 1s, then 10s for each of up to 25 answers.
    const CASE_MS: i64 = 251_000;
    let start_ms = clock::now_ms();
    let mut cases = Map::new();
    for (idx, order) in orders.iter().enumerate() {
        for reschedule in [true, false] {
            let case_number = idx as i64 * 2 + i64::from(!reschedule);
            let case_seed = config.seed ^ 1 ^ case_number as u64;
            clock::set_ms(start_ms + case_number * CASE_MS + 1000);
            let name = format!("golden {idx} {reschedule}");
            let path = harness.open_copy(source, &name)?;
            cases.insert(
                format!("{order} reschedule={reschedule}"),
                filtered_case(harness, case_seed, &name, reschedule, order)?,
            );
            harness.close()?;
            fs::remove_file(path)?;
        }
    }
    output.insert("filtered".into(), Value::Object(cases));
    clock::set_ms(start_ms + orders.len() as i64 * 2 * CASE_MS);

    let path = harness.open_copy(source, "changes")?;
    let review = harness.search("is:review -is:suspended")?;
    let new = harness.search("is:new")?;
    let learning = harness.search("is:learn")?;
    let review_a = sample(&review, 20, 1);
    let new_a = sample(&new, 10, 2);
    let learn_a = sample(&learning, 10, 3);
    let mixed: Vec<i64> = [&review_a[..], &new_a[..], &learn_a[..]].concat();
    let mut changes = Map::new();
    for days in ["0", "7", "3-9", "5!", "0!", "1-3!"] {
        changes.insert(
            format!("set_due {days}"),
            change_case(
                harness,
                "SetDueDate",
                json!({ "card_ids": mixed, "days": days }),
                &mixed,
            ),
        );
        clock::advance_ms(1000);
    }
    for (log, restore, reset) in [
        (true, false, false),
        (false, false, false),
        (true, true, false),
        (true, false, true),
        (true, true, true),
    ] {
        changes.insert(
            format!("forget log={log} restore_position={restore} reset_counts={reset}"),
            change_case(
                harness,
                "ScheduleCardsAsNew",
                json!({
                    "card_ids": mixed,
                    "log": log,
                    "restore_position": restore,
                    "reset_counts": reset,
                }),
                &mixed,
            ),
        );
        clock::advance_ms(1000);
    }
    for (name, _) in RATINGS {
        changes.insert(
            format!("grade_now {name}"),
            change_case(
                harness,
                "GradeNow",
                json!({ "card_ids": mixed, "rating": name }),
                &mixed,
            ),
        );
        clock::advance_ms(1000);
    }
    changes.insert(
        "suspend".into(),
        change_case(
            harness,
            "SuspendCards",
            json!({ "card_ids": mixed }),
            &mixed,
        ),
    );
    let suspended = sample(&harness.search("is:suspended")?, 10, 4);
    changes.insert(
        "unsuspend".into(),
        change_case(
            harness,
            "UnsuspendCards",
            json!({ "cids": suspended }),
            &suspended,
        ),
    );
    changes.insert(
        "sort".into(),
        change_case(
            harness,
            "SortCards",
            json!({
                "card_ids": new_a,
                "starting_from": 1,
                "step_size": 1,
                "randomize": false,
                "shift_existing": true,
            }),
            &new_a,
        ),
    );
    output.insert("changes".into(), Value::Object(changes));
    harness.close()?;
    fs::remove_file(path)?;

    for reschedule in [true, false] {
        output.insert(
            format!("recompute_with_reschedule_{reschedule}"),
            recompute_case(harness, source, reschedule)?,
        );
    }
    Ok(output)
}

/// A retention change is what makes the backend recompute memory states and,
/// if asked, reschedule. The settings are round-tripped as data, and if their
/// shape has changed that's recorded rather than worked around.
fn recompute_case(harness: &mut Harness, source: &Path, reschedule: bool) -> Result<Value> {
    let path = harness.open_copy(source, "recompute")?;
    let (did, _, cards) = biggest_deck(harness)?;
    let settings = harness.record("GetDeckConfigsForUpdate", json!({ "did": did }));
    let mut config = settings["config"].clone();
    let result = if config.is_object() {
        config["config"]["desired_retention"] = 0.85.into();
        harness.record(
            "UpdateDeckConfigs",
            json!({
                "target_deck_id": did,
                "config": config,
                "mode": "UPDATE_DECK_CONFIGS_MODE_NORMAL",
                "limits": settings["current_deck"]["limits"],
                "fsrs_reschedule": reschedule,
            }),
        )
    } else {
        json!({ "error": "the deck's settings aren't where this case expects" })
    };
    let output = json!({
        "error": result["error"],
        "cards": harness.cards(&cards),
        "stats": harness.record("Graphs", json!({ "search": "", "days": 365 })),
    });
    harness.close()?;
    fs::remove_file(path)?;
    Ok(output)
}

fn tools(harness: &mut Harness, source: &Path, _: &Config) -> Result<Map<String, Value>> {
    let path = harness.open_copy(source, "tools")?;
    let (_, name, cards) = biggest_deck(harness)?;
    let search = deck_search(&name);
    let mut params = Value::Array(vec![]);
    for cid in &cards {
        let found = harness.record("CardStats", json!({ "cid": cid }))["fsrs_params"].clone();
        if found.as_array().is_some_and(|list| !list.is_empty()) {
            params = found;
            break;
        }
    }
    let simulation = json!({
        "params": params,
        "desired_retention": 0.9,
        "deck_size": cards.len() + 500,
        "days_to_simulate": 365,
        "new_limit": 20,
        "review_limit": 200,
        "max_interval": 36500,
        "search": search,
        "new_cards_ignore_review_limit": true,
        "easy_days_percentages": [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "historical_retention": 0.9,
        "learning_step_count": 2,
        "relearning_step_count": 1,
    });
    let calls: [(&str, Value); 7] = [
        ("GetOptimalRetentionParameters", json!({ "search": search })),
        (
            "ComputeFsrsParams",
            json!({
                "search": search,
                "current_params": [],
                "num_of_relearning_steps": 1,
            }),
        ),
        (
            "EvaluateParams",
            json!({ "search": search, "num_of_relearning_steps": 1 }),
        ),
        (
            "EvaluateParamsLegacy",
            json!({ "params": params, "search": search }),
        ),
        ("SimulateFsrsReview", simulation.clone()),
        ("SimulateFsrsWorkload", simulation.clone()),
        ("ComputeOptimalRetention", simulation),
    ];
    let mut output = Map::new();
    output.insert("deck_cards".into(), cards.len().into());
    for (method, input) in calls {
        eprintln!("  {method}");
        output.insert(method.into(), harness.record(method, input));
    }
    harness.close()?;
    output.insert("rows_changed".into(), rows_changed(source, &path)?);
    fs::remove_file(path)?;
    Ok(output)
}

/// The rows that differ from the original in each table, as none should. The
/// original may be from before the usn column was removed.
fn rows_changed(original: &Path, after: &Path) -> Result<Value> {
    let without_usn = |mut rows: Value| {
        if let Some(rows) = rows.as_object_mut() {
            for row in rows.values_mut() {
                if let Some(row) = row.as_object_mut() {
                    row.remove("usn");
                }
            }
        }
        rows
    };
    let mut out = Map::new();
    for table in ["cards", "revlog", "deck_config"] {
        let left = without_usn(raw_rows(original, table, "")?);
        let right = without_usn(raw_rows(after, table, "")?);
        let (left, right) = (
            left.as_object().cloned().unwrap_or_default(),
            right.as_object().cloned().unwrap_or_default(),
        );
        let keys: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
        let changed: Vec<Value> = keys
            .iter()
            .filter(|key| left.get(key.as_str()) != right.get(key.as_str()))
            .map(
                |key| json!({ "before": left.get(key.as_str()), "after": right.get(key.as_str()) }),
            )
            .collect();
        out.insert(table.to_string(), changed.into());
    }
    Ok(Value::Object(out))
}
