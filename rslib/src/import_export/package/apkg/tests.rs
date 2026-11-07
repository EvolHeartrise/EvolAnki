// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#![cfg(test)]

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;

use anki_io::read_file;
use anki_proto::import_export::ImportAnkiPackageOptions;

use crate::card::CardQueue;
use crate::media::files::sha1_of_data;
use crate::media::MediaManager;
use crate::prelude::*;
use crate::search::SearchNode;
use crate::tests::open_fs_test_collection;
use crate::tests::DeckAdder;
use crate::tests::NoteAdder;

const SAMPLE_JPG: &str = "sample.jpg";
const SAMPLE_MP3: &str = "sample.mp3";
const SAMPLE_JS: &str = "_sample.js";
const JPG_DATA: &[u8] = b"1";
const MP3_DATA: &[u8] = b"2";
const JS_DATA: &[u8] = b"3";
const EXISTING_MP3_DATA: &[u8] = b"4";

/// Packages from elsewhere can hold buried cards, which arrive as normal ones.
#[test]
fn cards_buried_in_a_package_import_as_normal() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let exported = src_tempdir.path().join("exported.apkg");
    let apkg_path = src_tempdir.path().join("test.apkg");
    let review = NoteAdder::basic(&mut src_col)
        .fields(&["review", ""])
        .add(&mut src_col)
        .id;
    let new = NoteAdder::basic(&mut src_col)
        .fields(&["new", ""])
        .add(&mut src_col)
        .id;
    src_col.export_apkg(&exported, "", false).unwrap();

    // change the package's own database, as nothing can bury a card any more
    let mut archive = zip::ZipArchive::new(File::open(&exported).unwrap()).unwrap();
    let mut writer = zip::ZipWriter::new(File::create(&apkg_path).unwrap());
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().to_string();
        let mut bytes = vec![];
        std::io::copy(&mut entry, &mut bytes).unwrap();
        if name == "collection.anki21b" {
            let db_path = src_tempdir.path().join("package.db");
            std::fs::write(&db_path, zstd::decode_all(bytes.as_slice()).unwrap()).unwrap();
            let db = rusqlite::Connection::open(&db_path).unwrap();
            db.execute("update cards set queue = -2 where nid = ?", [new.0])
                .unwrap();
            db.execute(
                "update cards set queue = -3, type = 2, due = 9, ivl = 20 where nid = ?",
                [review.0],
            )
            .unwrap();
            db.close().unwrap();
            bytes = zstd::encode_all(std::fs::read(&db_path).unwrap().as_slice(), 0).unwrap();
        }
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap();

    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                ..Default::default()
            },
        )
        .unwrap();

    let queues: HashMap<NoteId, CardQueue> = target_col
        .storage
        .get_all_cards()
        .into_iter()
        .map(|card| (card.note_id, card.queue))
        .collect();
    assert_eq!(queues[&review], CardQueue::Review);
    assert_eq!(queues[&new], CardQueue::New);
}

#[test]
fn roundtrip() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let apkg_path = src_tempdir.path().join("test.apkg");

    let (main_deck, sibling_deck) = src_col.add_sample_decks();
    let notetype = src_col.add_sample_notetype();
    let note = src_col.add_sample_note(&main_deck, &sibling_deck, &notetype);
    src_col.add_sample_media();
    target_col.add_conflicting_media();

    src_col
        .export_apkg(
            &apkg_path,
            SearchNode::from_deck_name("parent::sample"),
            true,
        )
        .unwrap();
    target_col
        .import_apkg(&apkg_path, ImportAnkiPackageOptions::default())
        .unwrap();

    target_col.assert_decks();
    target_col.assert_notetype(&notetype);
    target_col.assert_note_and_media(&note);

    target_col.undo().unwrap();
    target_col.assert_empty();
}

impl Collection {
    fn add_sample_decks(&mut self) -> (Deck, Deck) {
        let sample = self.add_named_deck("parent\x1fsample");
        self.add_named_deck("parent\x1fsample\x1fchild");
        let siblings = self.add_named_deck("siblings");

        (sample, siblings)
    }

    fn add_named_deck(&mut self, name: &str) -> Deck {
        let mut deck = Deck::new_normal();
        deck.name = NativeDeckName::from_native_str(name);
        self.add_deck(&mut deck).unwrap();
        deck
    }

    fn add_sample_notetype(&mut self) -> Notetype {
        let mut nt = Notetype {
            name: "sample".into(),
            ..Default::default()
        };
        nt.add_field("sample");
        nt.add_template("sample1", "{{sample}}", "<script src=_sample.js></script>");
        nt.add_template("sample2", "{{sample}}2", "");
        self.add_notetype(&mut nt, true).unwrap();
        nt
    }

    fn add_sample_note(
        &mut self,
        main_deck: &Deck,
        sibling_decks: &Deck,
        notetype: &Notetype,
    ) -> Note {
        let mut sample = notetype.new_note();
        sample.fields_mut()[0] = format!("<img src='{SAMPLE_JPG}'> [sound:{SAMPLE_MP3}]");
        sample.tags = vec!["sample".into()];
        self.add_note(&mut sample, main_deck.id).unwrap();

        let card = self
            .storage
            .get_card_by_ordinal(sample.id, 1)
            .unwrap()
            .unwrap();
        self.set_deck(&[card.id], sibling_decks.id).unwrap();

        sample
    }

    fn add_sample_media(&self) {
        self.add_media(&[
            (SAMPLE_JPG, JPG_DATA),
            (SAMPLE_MP3, MP3_DATA),
            (SAMPLE_JS, JS_DATA),
        ]);
    }

    fn add_conflicting_media(&mut self) {
        let mut file = File::create(self.media_folder.join(SAMPLE_MP3)).unwrap();
        file.write_all(EXISTING_MP3_DATA).unwrap();
    }

    fn assert_decks(&mut self) {
        let existing_decks: HashSet<_> = self
            .get_all_deck_names(true)
            .unwrap()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        for deck in ["parent", "parent::sample", "siblings"] {
            assert!(existing_decks.contains(deck));
        }
        assert!(!existing_decks.contains("parent::sample::child"));
    }

    fn assert_notetype(&mut self, notetype: &Notetype) {
        assert!(self.get_notetype(notetype.id).unwrap().is_some());
    }

    fn assert_note_and_media(&mut self, note: &Note) {
        let sha1 = sha1_of_data(MP3_DATA);
        let new_mp3_name = format!("sample-{}.mp3", hex::encode(sha1));
        let csums = MediaManager::new(&self.media_folder, &self.media_db)
            .unwrap()
            .all_checksums_as_is();

        for (fname, orig_data) in [
            (SAMPLE_JPG, JPG_DATA),
            (SAMPLE_MP3, EXISTING_MP3_DATA),
            (new_mp3_name.as_str(), MP3_DATA),
            (SAMPLE_JS, JS_DATA),
        ] {
            // data should have been copied correctly
            assert_eq!(read_file(self.media_folder.join(fname)).unwrap(), orig_data);
            // and checksums in media db should be valid
            assert_eq!(*csums.get(fname).unwrap(), sha1_of_data(orig_data));
        }

        let imported_note = self.storage.get_note(note.id).unwrap().unwrap();
        assert!(imported_note.fields()[0].contains(&new_mp3_name));
    }

    fn assert_empty(&self) {
        assert!(self.get_all_deck_names(true).unwrap().is_empty());
        assert!(self.storage.get_all_note_ids().unwrap().is_empty());
        assert!(self.storage.get_all_card_ids().unwrap().is_empty());
        assert!(self.storage.all_tags().unwrap().is_empty());
    }
}

#[test]
fn fsrs_params_in_a_package_are_imported_with_scheduling() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let apkg_path = src_tempdir.path().join("fsrs.apkg");

    let deck = DeckAdder::new("fsrs-deck")
        .with_config(|c| {
            c.inner.fsrs_params_4 = vec![0.1; 17];
            c.inner.fsrs_params_5 = vec![0.2; 19];
            c.inner.fsrs_params_6 = vec![0.3; 21];
        })
        .add(&mut src_col);
    NoteAdder::basic(&mut src_col)
        .deck(deck.id)
        .add(&mut src_col);

    src_col
        .export_apkg(&apkg_path, SearchNode::WholeCollection, false)
        .unwrap();
    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                with_deck_configs: true,
                ..Default::default()
            },
        )
        .unwrap();

    let deck = target_col.get_deck_id("fsrs-deck").unwrap().unwrap();
    let conf = target_col
        .get_deck_config(DeckConfigId(deck.0), false)
        .unwrap()
        .expect("config should have been imported");
    assert_eq!(conf.inner.fsrs_params_4.len(), 17);
    assert_eq!(conf.inner.fsrs_params_5.len(), 19);
    assert_eq!(conf.inner.fsrs_params_6.len(), 21);
}

/// What an older version or client could have written: `b` uses `a`'s settings,
/// and has an override of its own.
fn package_with_shared_settings(path: &std::path::Path, src_col: &mut Collection) {
    let a = DeckAdder::new("a")
        .with_config(|c| {
            c.inner.new_per_day = 11;
            c.inner.reviews_per_day = 111;
        })
        .add(src_col);
    let mut b = DeckAdder::new("b")
        .with_config(|c| c.inner.new_per_day = 99)
        .add(src_col);
    NoteAdder::basic(src_col).deck(a.id).add(src_col);
    NoteAdder::basic(src_col).deck(b.id).add(src_col);
    let normal = b.normal_mut().unwrap();
    normal.config_id = a.id.0;
    normal.new_limit = Some(7);
    src_col.storage.update_deck(&b).unwrap();
    src_col
        .export_apkg(path, SearchNode::WholeCollection, false)
        .unwrap();
}

fn settings_of(col: &mut Collection, name: &str) -> DeckConfig {
    let deck = col.get_deck_id(name).unwrap().unwrap();
    col.get_deck_config(DeckConfigId(deck.0), false)
        .unwrap()
        .unwrap_or_else(|| panic!("{name} has no settings"))
}

#[test]
fn decks_sharing_settings_in_a_package_each_get_their_own_copy() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let apkg_path = src_tempdir.path().join("shared.apkg");
    package_with_shared_settings(&apkg_path, &mut src_col);

    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                with_deck_configs: true,
                ..Default::default()
            },
        )
        .unwrap();

    let a = settings_of(&mut target_col, "a");
    let b = settings_of(&mut target_col, "b");
    assert_ne!(a.id, b.id);
    assert_eq!(a.inner.new_per_day, 11);
    assert_eq!(a.inner.reviews_per_day, 111);
    // what it was using, with its override
    assert_eq!(b.inner.new_per_day, 7);
    assert_eq!(b.inner.reviews_per_day, 111);

    // the file's own ids don't come along, and nothing is left unowned
    let owned = target_col.storage.get_all_decks().unwrap().len();
    assert_eq!(target_col.storage.all_deck_config().unwrap().len(), owned);

    // nor does importing the same thing again leave more
    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                with_deck_configs: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(target_col.storage.all_deck_config().unwrap().len(), owned);
}

#[test]
fn a_package_imported_without_settings_gives_built_in_values_or_keeps_the_decks_own() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let apkg_path = src_tempdir.path().join("shared.apkg");
    package_with_shared_settings(&apkg_path, &mut src_col);
    // an existing deck, and the local Default deck's settings, which a new
    // deck must not pick up
    let existing = DeckAdder::new("a")
        .with_config(|c| c.inner.new_per_day = 44)
        .add(&mut target_col);
    target_col.update_default_deck_config(|c| c.new_per_day = 55);

    target_col
        .import_apkg(&apkg_path, ImportAnkiPackageOptions::default())
        .unwrap();

    assert_eq!(settings_of(&mut target_col, "a").id.0, existing.id.0);
    assert_eq!(settings_of(&mut target_col, "a").inner.new_per_day, 44);
    assert_eq!(
        settings_of(&mut target_col, "b").inner.new_per_day,
        DeckConfig::default().inner.new_per_day
    );
    assert_eq!(
        settings_of(&mut target_col, "Default").inner.new_per_day,
        55
    );
}

/// A package in the format of stock Anki's "support older versions" export: no
/// meta, schema 11.
#[test]
fn a_package_with_schema_11_imports() {
    let (mut target_col, target_tempdir) = open_fs_test_collection("target");
    let apkg_path = target_tempdir.path().join("old.apkg");
    let mut writer = zip::ZipWriter::new(File::create(&apkg_path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("collection.anki2", options).unwrap();
    writer
        .write_all(include_bytes!("../../../../tests/support/mediacheck.anki2"))
        .unwrap();
    writer.start_file("media", options).unwrap();
    writer.write_all(b"{}").unwrap();
    writer.finish().unwrap();
    let notes_before = target_col.search_notes_unordered("").unwrap().len();

    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        target_col.search_notes_unordered("").unwrap().len(),
        notes_before + 3
    );
    assert_eq!(target_col.storage.get_all_cards().len(), 3);
}

/// A package holding schema 18, as this and stock Anki used to write.
#[test]
fn a_package_with_schema_18_imports() {
    let (mut src_col, src_tempdir) = open_fs_test_collection("src");
    let (mut target_col, _target_tempdir) = open_fs_test_collection("target");
    let exported = src_tempdir.path().join("exported.apkg");
    let apkg_path = src_tempdir.path().join("test.apkg");
    let note = NoteAdder::basic(&mut src_col)
        .fields(&["in old package", ""])
        .add(&mut src_col)
        .id;
    src_col.export_apkg(&exported, "", false).unwrap();

    let mut archive = zip::ZipArchive::new(File::open(&exported).unwrap()).unwrap();
    let mut writer = zip::ZipWriter::new(File::create(&apkg_path).unwrap());
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().to_string();
        let mut bytes = vec![];
        std::io::copy(&mut entry, &mut bytes).unwrap();
        if name == "collection.anki21b" {
            let db_path = src_tempdir.path().join("package.db");
            std::fs::write(&db_path, zstd::decode_all(bytes.as_slice()).unwrap()).unwrap();
            let db = rusqlite::Connection::open(&db_path).unwrap();
            crate::storage::turn_into_schema_18(&db);
            db.close().unwrap();
            bytes = zstd::encode_all(std::fs::read(&db_path).unwrap().as_slice(), 0).unwrap();
        }
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap();

    target_col
        .import_apkg(
            &apkg_path,
            ImportAnkiPackageOptions {
                with_scheduling: true,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(target_col.storage.get_note(note).unwrap().is_some());
}
