// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anki::backend::Backend;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use evolanki_server::rpc;
use prost::Message;
use prost_reflect::DescriptorPool;
use prost_reflect::DynamicMessage;
use prost_reflect::MethodDescriptor;
use rusqlite::types::ValueRef;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;

use crate::clock;
use crate::json::float32;
use crate::json::hex;
use crate::json::json_to_message;
use crate::json::message_to_json;
use crate::json::splitmix;

pub const RATINGS: [(&str, &str); 4] = [
    ("AGAIN", "again"),
    ("HARD", "hard"),
    ("GOOD", "good"),
    ("EASY", "easy"),
];

pub struct Harness {
    backend: Backend,
    pool: DescriptorPool,
    pub skipped: BTreeSet<String>,
    work: PathBuf,
    open_count: u32,
    seed: u64,
    rpc_count: u64,
}

impl Harness {
    pub fn new(work: &Path) -> Result<Self> {
        let descriptors = include_bytes!(concat!(env!("OUT_DIR"), "/descriptors.bin"));
        fs::create_dir_all(work)?;
        Ok(Self {
            backend: rpc::new_backend()?,
            pool: DescriptorPool::decode(descriptors.as_ref())?,
            skipped: BTreeSet::new(),
            work: work.to_path_buf(),
            open_count: 0,
            seed: 0,
            rpc_count: 0,
        })
    }

    fn method(&self, name: &str) -> Option<MethodDescriptor> {
        self.pool
            .services()
            .flat_map(|service| service.methods().collect::<Vec<_>>())
            .find(|method| method.name() == name)
    }

    pub fn rpc(&mut self, name: &str, input: Value) -> Result<Value, String> {
        let method = self
            .method(name)
            .ok_or_else(|| format!("no such rpc: {name}"))?;
        let mut skipped = vec![];
        let request = json_to_message(&method.input(), &input, &mut skipped)
            .map_err(|err| format!("{name} request: {err}"))?;
        self.skipped.extend(skipped);
        let mut camel = name.to_string();
        camel[..1].make_ascii_lowercase();
        let encoded = request.encode_to_vec();
        self.rpc_count += 1;
        reset_randomness(splitmix(self.seed.wrapping_add(self.rpc_count)));
        // On a new thread each time, so that a thread's generator and hash keys
        // come from the seed above, and not from what earlier calls drew.
        let backend = &self.backend;
        let bytes = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(16 << 20)
                .spawn_scoped(scope, || rpc::call(backend, &camel, &encoded))
                .expect("spawning a thread")
                .join()
                .expect("rpc panicked")
        })
        .map_err(|err| err.message)?;
        let response = DynamicMessage::decode(method.output(), bytes.as_slice())
            .map_err(|err| format!("{name} response: {err}"))?;
        Ok(message_to_json(&response))
    }

    /// Calls after this draw randomness from `label` and their position after
    /// it. Keeps a part of a run independent of the parts before it.
    pub fn reseed(&mut self, label: &str) {
        self.seed = label.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        self.rpc_count = 0;
    }

    /// For calls the harness can't go on without.
    pub fn must(&mut self, name: &str, input: Value) -> Result<Value> {
        self.rpc(name, input)
            .map_err(|err| anyhow!("{name}: {err}"))
    }

    /// For behaviour being recorded, where failing is a result too.
    pub fn record(&mut self, name: &str, input: Value) -> Value {
        self.rpc(name, input)
            .unwrap_or_else(|err| json!({ "error": err }))
    }

    /// Work on a new copy of `source`. Randomness restarts by name, so a
    /// section's output doesn't depend on which ran before it.
    pub fn open_copy(&mut self, source: &Path, name: &str) -> Result<PathBuf> {
        self.open_count += 1;
        let target = self.work.join(format!("{name}-{}.anki2", self.open_count));
        fs::copy(source, &target)?;
        self.open_as(&target, name)?;
        Ok(target)
    }

    pub fn open(&mut self, path: &Path) -> Result<()> {
        let label = path.file_name().unwrap_or_default().to_string_lossy();
        self.open_as(path, &label)
    }

    fn open_as(&mut self, path: &Path, label: &str) -> Result<()> {
        self.reseed(label);
        let media = path.with_extension("media");
        fs::create_dir_all(&media)?;
        self.must(
            "OpenCollection",
            json!({
                "collection_path": path.to_str(),
                "media_folder_path": media.to_str(),
                "media_db_path": path.with_extension("mdb").to_str(),
            }),
        )?;
        Ok(())
    }

    pub fn close(&mut self) -> Result<()> {
        self.must("CloseCollection", json!({}))?;
        Ok(())
    }

    pub fn search(&mut self, query: &str) -> Result<Vec<i64>> {
        let response = self.must("SearchCards", json!({ "search": query }))?;
        let mut ids = ints(&response["ids"]);
        ids.sort_unstable();
        Ok(ids)
    }

    pub fn card(&mut self, cid: i64) -> Value {
        self.record("GetCard", json!({ "cid": cid }))
    }

    pub fn cards(&mut self, cids: &[i64]) -> Value {
        Value::Object(
            cids.iter()
                .map(|cid| (cid.to_string(), self.card(*cid)))
                .collect::<Map<_, _>>(),
        )
    }

    pub fn timing(&mut self) -> Result<Timing> {
        let response = self.must("SchedTimingToday", json!({}))?;
        Ok(Timing {
            today: response["days_elapsed"].as_i64().unwrap_or(0),
            next_day_at: response["next_day_at"].as_i64().unwrap_or(0),
            now: clock::now_secs(),
        })
    }

    /// The newest revlog row as the stats RPC reports it, and how many there
    /// are.
    pub fn latest_revlog(&mut self, cid: i64) -> Value {
        let response = self.record("GetReviewLogs", json!({ "cid": cid }));
        let entries = response["entries"].as_array().cloned().unwrap_or_default();
        let mut latest = Value::Null;
        let mut latest_time = i64::MIN;
        for entry in &entries {
            let time = entry["time"].as_i64().unwrap_or(0);
            if time > latest_time {
                latest_time = time;
                latest = entry.clone();
            }
        }
        json!({ "count": entries.len(), "latest": latest })
    }

    /// Everything a person reviewing the card would be told about it.
    pub fn card_states(&mut self, cid: i64) -> Value {
        let memory = self.record("ComputeMemoryState", json!({ "cid": cid }));
        let raw = match self.rpc("GetSchedulingStates", json!({ "cid": cid })) {
            Ok(raw) => raw,
            Err(err) => return json!({ "states_error": err, "memory": memory }),
        };
        let labels = self.record("DescribeNextStates", raw.clone());
        let context = match self.timing() {
            Ok(timing) => StateContext {
                timing,
                desired_retention: memory["desired_retention"].as_f64().unwrap_or(0.0) as f32,
                decay: memory["decay"].as_f64().unwrap_or(0.0) as f32,
            },
            Err(err) => return json!({ "states_error": err.to_string() }),
        };
        json!({
            "states": flat_states(&raw, &context),
            "labels": labels["vals"],
            "memory": memory,
        })
    }

    pub fn answer(
        &mut self,
        cid: i64,
        states: &Value,
        rating: usize,
        millis: u64,
    ) -> Result<Value, String> {
        let (name, key) = RATINGS[rating];
        self.rpc(
            "AnswerCard",
            json!({
                "card_id": cid,
                "current_state": states["current"],
                "new_state": states[key],
                "rating": name,
                "answered_at_millis": clock::now_ms(),
                "milliseconds_taken": millis,
            }),
        )
    }
}

fn reset_randomness(seed: u64) {
    clock::reset_random(seed);
    const SQLITE_TESTCTRL_PRNG_SEED: i32 = 28;
    // SAFETY: a null database means the seed is used as given. SQLite treats
    // this as a global setting, and there's no other SQLite use at this point.
    unsafe {
        rusqlite::ffi::sqlite3_test_control(
            SQLITE_TESTCTRL_PRNG_SEED,
            seed as i32,
            std::ptr::null_mut::<rusqlite::ffi::sqlite3>(),
        );
    }
}

pub fn ints(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

#[derive(Clone, Copy)]
pub struct Timing {
    pub today: i64,
    pub next_day_at: i64,
    pub now: i64,
}

pub struct StateContext {
    pub timing: Timing,
    pub desired_retention: f32,
    pub decay: f32,
}

pub fn flat_states(states: &Value, context: &StateContext) -> Value {
    let mut out = Map::new();
    for key in ["current", "again", "hard", "good", "easy"] {
        out.insert(key.to_string(), flat_state(&states[key], context));
    }
    Value::Object(out)
}

/// One state as a flat record. `due_estimate` and `fuzz` are the harness's own
/// arithmetic on what the backend reports, as it doesn't expose them.
pub fn flat_state(state: &Value, context: &StateContext) -> Value {
    let mut out = Map::new();
    let mut normal = &state["normal"];
    if let Some(filtered) = state.get("filtered") {
        if let Some(preview) = filtered.get("preview") {
            out.insert("kind".into(), "preview".into());
            out.insert("scheduled_secs".into(), or_zero(&preview["scheduled_secs"]));
            out.insert("finished".into(), or_false(&preview["finished"]));
            return Value::Object(out);
        }
        out.insert("in_rescheduling_filter".into(), true.into());
        normal = &filtered["rescheduling"]["original_state"];
    }
    if let Some(new) = normal.get("new") {
        out.insert("kind".into(), "new".into());
        out.insert("position".into(), or_zero(&new["position"]));
    } else if let Some(learning) = normal.get("learning") {
        out.insert("kind".into(), "learning".into());
        add_learning(&mut out, learning, context);
    } else if let Some(review) = normal.get("review") {
        out.insert("kind".into(), "review".into());
        add_review(&mut out, review, context);
    } else if let Some(relearning) = normal.get("relearning") {
        out.insert("kind".into(), "relearning".into());
        add_review(&mut out, &relearning["review"], context);
        add_learning(&mut out, &relearning["learning"], context);
    } else {
        out.insert("kind".into(), "unknown".into());
    }
    Value::Object(out)
}

fn or_zero(value: &Value) -> Value {
    if value.is_null() {
        0.into()
    } else {
        value.clone()
    }
}

fn or_false(value: &Value) -> Value {
    if value.is_null() {
        false.into()
    } else {
        value.clone()
    }
}

fn add_memory(out: &mut Map<String, Value>, state: &Value) {
    if let Some(memory) = state.get("memory_state") {
        out.insert("stability".into(), or_zero(&memory["stability"]));
        out.insert("difficulty".into(), or_zero(&memory["difficulty"]));
    }
}

fn add_learning(out: &mut Map<String, Value>, learning: &Value, context: &StateContext) {
    let secs = learning["scheduled_secs"].as_i64().unwrap_or(0);
    out.insert(
        "remaining_steps".into(),
        or_zero(&learning["remaining_steps"]),
    );
    out.insert("interval_secs".into(), secs.into());
    out.insert("elapsed_secs".into(), or_zero(&learning["elapsed_secs"]));
    let timing = context.timing;
    let due = if timing.now + secs < timing.next_day_at {
        json!({ "timestamp": timing.now + secs })
    } else {
        // the same day-count rule the backend uses for its labels
        let days = (secs - (timing.next_day_at - timing.now)) / 86_400 + 1;
        json!({ "day": timing.today + days })
    };
    out.insert("due_estimate".into(), due);
    add_memory(out, learning);
}

fn add_review(out: &mut Map<String, Value>, review: &Value, context: &StateContext) {
    let days = review["scheduled_days"].as_i64().unwrap_or(0);
    out.insert("interval_days".into(), days.into());
    out.insert("elapsed_days".into(), or_zero(&review["elapsed_days"]));
    out.insert("lapses".into(), or_zero(&review["lapses"]));
    // left as reported, and missing if the backend stops reporting it
    out.insert(
        "due_estimate".into(),
        json!({ "day": context.timing.today + days }),
    );
    add_memory(out, review);
    if let Some(stability) = review["memory_state"]["stability"].as_f64() {
        if context.desired_retention > 0.0 && context.decay != 0.0 {
            let ideal = ideal_interval(stability as f32, context.desired_retention, context.decay);
            let (lower, upper) = fuzz_bounds(ideal);
            out.insert(
                "fuzz".into(),
                json!({
                    "ideal": float32(ideal),
                    "lower": lower,
                    "upper": upper,
                    "in_range": (lower..=upper).contains(&(days as u32)),
                }),
            );
        }
    }
}

fn ideal_interval(stability: f32, retention: f32, decay: f32) -> f32 {
    let decay = -decay.abs();
    let factor = 0.9f32.powf(1.0 / decay) - 1.0;
    stability / factor * (retention.powf(1.0 / decay) - 1.0)
}

/// An independent copy of the range in states/fuzz.rs, for intervals from 1
/// day, before any minimum raised by the previous interval.
fn fuzz_bounds(interval: f32) -> (u32, u32) {
    let interval = interval.clamp(1.0, 36500.0);
    let delta = if interval < 2.5 {
        0.0
    } else {
        1.0 + 0.15 * (interval.min(7.0) - 2.5).max(0.0)
            + 0.1 * (interval.min(20.0) - 7.0).max(0.0)
            + 0.05 * (interval - 20.0).max(0.0)
    };
    (
        ((interval - delta).round() as u32).max(1),
        (interval + delta).round() as u32,
    )
}

/// Each row of a table, keyed by its first column.
pub fn raw_rows(path: &Path, table: &str, filter: &str) -> Result<Value> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = db.prepare(&format!("select * from {table} {filter}"))?;
    let names: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = statement.query([])?;
    let mut out = Map::new();
    while let Some(row) = rows.next()? {
        let mut item = Map::new();
        for (idx, name) in names.iter().enumerate() {
            item.insert(
                name.clone(),
                match row.get_ref(idx)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(val) => val.into(),
                    ValueRef::Real(val) => crate::json::float(val),
                    ValueRef::Text(val) => String::from_utf8_lossy(val).to_string().into(),
                    ValueRef::Blob(val) => hex(val).into(),
                },
            );
        }
        let key = item
            .get(&names[0])
            .map(|val| match val {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        out.insert(key, Value::Object(item));
    }
    Ok(Value::Object(out))
}

/// Refuse paths this tool must never touch, whatever it's asked.
pub fn check_input_path(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    let home = std::env::var("HOME").unwrap_or_default();
    for forbidden in [".local/share/evolanki", ".local/share/Anki2"] {
        if home.is_empty() {
            bail!("HOME isn't set, so the live data folders can't be ruled out");
        }
        let folder = Path::new(&home).join(forbidden);
        let resolved = fs::canonicalize(&folder).unwrap_or(folder);
        if path.starts_with(resolved) {
            bail!("refusing {}: copy it elsewhere first", path.display());
        }
    }
    Ok(path)
}

/// Output can hold card content, so must not land in a repository or a data
/// folder.
pub fn prepare_output(path: &Path) -> Result<PathBuf> {
    // checked before anything is created, through the part that exists already
    let absolute = std::path::absolute(path)?;
    let existing = absolute
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .ok_or_else(|| anyhow!("no existing parent for {}", path.display()))?;
    let resolved = check_input_path(existing)?;
    for ancestor in resolved.ancestors() {
        if ancestor.join(".git").exists() {
            bail!("refusing {}: inside a git repository", path.display());
        }
    }
    fs::create_dir_all(path)?;
    let path = check_input_path(path)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}
