// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Compares two output folders, summarising by the shape of the path, so that
//! one changed field across every card reads as one line.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

#[derive(Default)]
struct Summary {
    count: usize,
    examples: Vec<String>,
}

/// Returns true if there were differences.
pub fn diff_dirs(a: &Path, b: &Path, ignore: &BTreeSet<String>, examples: usize) -> Result<bool> {
    let mut names = BTreeSet::new();
    for dir in [a, b] {
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                names.insert(name);
            }
        }
    }
    let mut found = false;
    for name in names {
        let (Ok(left), Ok(right)) = (fs::read(a.join(&name)), fs::read(b.join(&name))) else {
            println!("{name}: only in one folder");
            found = true;
            continue;
        };
        let left: Value = serde_json::from_slice(&left)?;
        let right: Value = serde_json::from_slice(&right)?;
        let mut summary = BTreeMap::new();
        walk(
            &left,
            &right,
            name.clone(),
            String::new(),
            ignore,
            examples,
            &mut summary,
        );
        if summary.is_empty() {
            println!("{name}: identical");
        } else {
            found = true;
            let total: usize = summary.values().map(|s: &Summary| s.count).sum();
            println!("{name}: {total} differences");
            for (shape, entry) in summary {
                println!("  {:>8}  {shape}", entry.count);
                for example in entry.examples {
                    println!("            {example}");
                }
            }
        }
    }
    Ok(found)
}

fn walk(
    left: &Value,
    right: &Value,
    path: String,
    shape: String,
    ignore: &BTreeSet<String>,
    examples: usize,
    out: &mut BTreeMap<String, Summary>,
) {
    match (left, right) {
        (Value::Object(l), Value::Object(r)) => {
            let keys: BTreeSet<_> = l.keys().chain(r.keys()).collect();
            for key in keys {
                let part = if key.bytes().all(|b| b.is_ascii_digit()) {
                    "#".to_string()
                } else {
                    key.clone()
                };
                let child_shape = format!("{shape}.{part}");
                // a bare key anywhere, or a dotted path by how it ends
                if ignore.contains(key.as_str())
                    || ignore
                        .iter()
                        .any(|item| item.contains('.') && child_shape.ends_with(item.as_str()))
                {
                    continue;
                }
                walk(
                    l.get(key).unwrap_or(&Value::Null),
                    r.get(key).unwrap_or(&Value::Null),
                    format!("{path}.{key}"),
                    child_shape,
                    ignore,
                    examples,
                    out,
                );
            }
        }
        (Value::Array(l), Value::Array(r)) if l.len() == r.len() => {
            for (idx, (a, b)) in l.iter().zip(r).enumerate() {
                walk(
                    a,
                    b,
                    format!("{path}[{idx}]"),
                    format!("{shape}[]"),
                    ignore,
                    examples,
                    out,
                );
            }
        }
        (a, b) if a == b => {}
        (a, b) => {
            let entry = out.entry(shape).or_default();
            entry.count += 1;
            if entry.examples.len() < examples {
                let mut text = format!("{path}: {a} -> {b}");
                if text.len() > 300 {
                    text = text.chars().take(300).collect::<String>() + "...";
                }
                entry.examples.push(text);
            }
        }
    }
}
