// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The English strings in ftl/core are the only ones there are.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub type TranslationsByRepo = HashMap<String, String>;
pub type TranslationsByFile = HashMap<String, TranslationsByRepo>;
pub type TranslationsByLang = HashMap<String, TranslationsByFile>;

/// Read the contents of the FTL files into a TranslationMap structure.
pub fn get_ftl_data() -> TranslationsByLang {
    let mut map = TranslationsByLang::default();

    add_folder(
        &mut map,
        &source_tree_root().join("core"),
        "core",
        "templates",
    );
    map
}

/// For each .ftl file in the provided folder, add it to the translation map.
fn add_folder(map: &mut TranslationsByLang, folder: &Path, repo: &str, lang: &str) {
    let map_entry = map.entry(lang.to_string()).or_default();
    for entry in fs::read_dir(folder).unwrap() {
        let entry = entry.unwrap();
        let fname = entry.file_name().to_string_lossy().to_string();
        if !fname.ends_with(".ftl") {
            continue;
        }
        let module = fname.trim_end_matches(".ftl").replace('-', "_");
        let text = fs::read_to_string(entry.path()).unwrap();
        assert!(
            text.ends_with('\n'),
            "file was missing final newline: {entry:?}"
        );
        map_entry
            .entry(module)
            .or_default()
            .entry(repo.to_string())
            .or_default()
            .push_str(&text);
        println!("cargo:rerun-if-changed={}", entry.path().to_str().unwrap());
    }
}

fn source_tree_root() -> PathBuf {
    PathBuf::from("../../ftl")
}
