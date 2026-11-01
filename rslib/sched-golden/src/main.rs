// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Records what the scheduler does as JSON, with time and randomness pinned, so
//! that two builds can be diffed. See `--help`.
//!
//! This only uses RPCs, looked up by name at runtime, and raw table reads that
//! select whatever columns exist. Nothing here names a scheduler setting or a
//! generated type, so it keeps compiling as the backend changes.

#[cfg(target_os = "linux")]
mod clock;
#[cfg(target_os = "linux")]
mod diff;
#[cfg(target_os = "linux")]
mod harness;
#[cfg(target_os = "linux")]
mod json;
#[cfg(target_os = "linux")]
mod sections;
#[cfg(target_os = "linux")]
mod synthetic;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("sched-golden pins time through libc symbols, which needs Linux");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
const HELP: &str = "\
sched-golden run --collection COPY.anki2 --out DIR [--now SECS] [--seed N]
                 [--answers N] [--days N] [--only snapshot,replay,ops,tools]
sched-golden synthetic --fixtures DIR --out DIR [--now SECS]
sched-golden diff DIR_A DIR_B [--ignore key,key] [--examples N]

The collection is copied before it is opened, and never written. The live data
folders are refused. Output can contain card content: keep it out of git, which
is why --out is refused inside a repository. Run release builds, as the optimizer
and simulator are slow otherwise, and compare like with like.

--ignore takes key names, which match anywhere, or dotted paths matched by how
they end, with # for any card id: final_cards.#.factor";

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    // Before anything reads the zone, and while there is only this thread.
    std::env::set_var("TZ", "UTC");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("--help");
    let mut positional = vec![];
    let mut options = BTreeMap::new();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(name) = arg.strip_prefix("--") {
            let value = iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("--{name} needs a value"))?;
            options.insert(name.to_string(), value.clone());
        } else {
            positional.push(arg.clone());
        }
    }
    let number = |name: &str, default: i64| -> anyhow::Result<i64> {
        Ok(match options.get(name) {
            Some(value) => value.parse()?,
            None => default,
        })
    };
    let path = |name: &str| -> anyhow::Result<PathBuf> {
        options
            .get(name)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("--{name} is required\n\n{HELP}"))
    };
    // 2026-11-01 20:00:00 UTC
    let now = number("now", 1_793_563_200)?;
    clock::set_ms(now * 1000);
    clock::verify();

    match command {
        "run" => {
            let config = sections::Config {
                collection: path("collection")?,
                out: path("out")?,
                now,
                seed: number("seed", 20_261_101)? as u64,
                answers: number("answers", 6000)? as usize,
                days: number("days", 21)? as usize,
                only: options
                    .get("only")
                    .map(String::as_str)
                    .unwrap_or("snapshot,replay,ops,tools")
                    .split(',')
                    .map(str::to_string)
                    .collect(),
            };
            sections::run(&config)
        }
        "synthetic" => synthetic::run(&path("fixtures")?, &path("out")?, now),
        "diff" => {
            let [a, b] = positional.as_slice() else {
                anyhow::bail!("{HELP}");
            };
            let ignore: BTreeSet<String> = options
                .get("ignore")
                .map(|keys| keys.split(',').map(str::to_string).collect())
                .unwrap_or_default();
            let different = diff::diff_dirs(
                a.as_ref(),
                b.as_ref(),
                &ignore,
                number("examples", 3)? as usize,
            )?;
            std::process::exit(different as i32);
        }
        _ => {
            println!("{HELP}");
            Ok(())
        }
    }
}
