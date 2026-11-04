// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The English strings in ftl/core are the only ones there are.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// The text of each .ftl file in ftl/core, keyed by module name.
pub type TranslationsByModule = BTreeMap<String, String>;

pub fn get_ftl_data() -> TranslationsByModule {
    let mut map = TranslationsByModule::default();
    let folder = PathBuf::from("../../ftl/core");
    for entry in fs::read_dir(&folder).unwrap() {
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
        map.insert(module, text);
        println!("cargo:rerun-if-changed={}", entry.path().to_str().unwrap());
    }
    map
}
