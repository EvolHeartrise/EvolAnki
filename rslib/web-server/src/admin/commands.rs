// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The admin commands. Each one maps JSON onto backend RPCs, as those are what
//! keep the undo queue, FSRS state and deck counts consistent; none writes to
//! the database itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anki::backend::Backend;
use anki::media::files::filename_if_normalized;
use anki_proto::card_rendering::rendered_template_node;
use anki_proto::card_rendering::EmptyCardsReport;
use anki_proto::card_rendering::RenderCardResponse;
use anki_proto::card_rendering::RenderExistingCardRequest;
use anki_proto::card_rendering::RenderedTemplateNode;
use anki_proto::cards::Card;
use anki_proto::cards::CardId;
use anki_proto::cards::CardIds;
use anki_proto::cards::RemoveCardsRequest;
use anki_proto::cards::SetDeckRequest;
use anki_proto::collection::CheckDatabaseResponse;
use anki_proto::collection::OpChanges;
use anki_proto::collection::OpChangesWithCount;
use anki_proto::collection::OpChangesWithId;
use anki_proto::collection::UndoStatus;
use anki_proto::decks::deck::filtered::search_term::Order;
use anki_proto::decks::deck::filtered::SearchTerm;
use anki_proto::decks::Deck;
use anki_proto::decks::DeckId;
use anki_proto::decks::DeckIds;
use anki_proto::decks::DeckTreeNode;
use anki_proto::decks::DeckTreeRequest;
use anki_proto::decks::FilteredDeckForUpdate;
use anki_proto::decks::RenameDeckRequest;
use anki_proto::generic;
use anki_proto::import_export::csv_metadata;
use anki_proto::import_export::CsvMetadata;
use anki_proto::import_export::CsvMetadataRequest;
use anki_proto::import_export::ImportAnkiPackageOptions;
use anki_proto::import_export::ImportAnkiPackageRequest;
use anki_proto::import_export::ImportAnkiPackageUpdateCondition;
use anki_proto::import_export::ImportCollectionPackageRequest;
use anki_proto::import_export::ImportCsvRequest;
use anki_proto::import_export::ImportResponse;
use anki_proto::media::AddMediaFileRequest;
use anki_proto::media::CheckMediaResponse;
use anki_proto::notes::AddNoteRequest;
use anki_proto::notes::AddNoteResponse;
use anki_proto::notes::Note;
use anki_proto::notes::NoteFieldsCheckResponse;
use anki_proto::notes::NoteId;
use anki_proto::notes::RemoveNotesRequest;
use anki_proto::notes::UpdateNotesRequest;
use anki_proto::notetypes::notetype;
use anki_proto::notetypes::Notetype;
use anki_proto::notetypes::NotetypeId;
use anki_proto::notetypes::NotetypeUseCounts;
use anki_proto::scheduler::custom_study_request::cram::CramKind;
use anki_proto::scheduler::custom_study_request::Cram;
use anki_proto::scheduler::custom_study_request::Value as StudyValue;
use anki_proto::scheduler::CustomStudyRequest;
use anki_proto::scheduler::SchedTimingTodayResponse;
use anki_proto::scheduler::SetDueDateRequest;
use anki_proto::scheduler::SuspendCardsRequest;
use anki_proto::search::SearchRequest;
use anki_proto::search::SearchResponse;
use anki_proto::stats::CardStatsResponse;
use anki_proto::tags::NoteIdsAndTagsRequest;
use anki_proto::tags::RenameTagsRequest;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use chrono::Local;
use serde_json::json;
use serde_json::Value;

use super::Admin;
use super::Command;
use super::Effect;
use super::Effect::Destructive;
use super::Effect::Read;
use crate::collection::close_collection;
use crate::collection::media_paths;
use crate::collection::open_collection;
use crate::collection::utf8;
use crate::rpc::call_typed;

const IDS: &str = r#"either "ids":[1,2] or "search":"anki search""#;

macro_rules! command {
    ($name:literal, $effect:expr, $args:expr, $help:literal, $run:expr) => {
        Command {
            name: $name,
            effect: $effect,
            args: $args,
            help: $help,
            run: $run,
        }
    };
}

pub const COMMANDS: &[Command] = &[
    command!("search.notes", Read, r#"{"query":"deck:x tag:y","limit":N?}"#,
        "Note ids matching an Anki search. An empty query matches everything.", search_notes),
    command!("search.cards", Read, r#"{"query":"is:due","limit":N?}"#,
        "Card ids matching an Anki search.", search_cards),
    command!("note.show", Read, r#"{"id":N}"#, "A note's fields, tags and cards.", note_show),
    command!("card.show", Read, r#"{"id":N,"render":bool?}"#,
        "A card's scheduling, memory state and rendered sides.", card_show),
    command!("note.add", Effect::Write,
        r#"{"notetype":"Basic","deck":"Default","fields":{"Front":"html"},"tags":["t"]?,"allow_duplicate":false?}"#,
        "Add a note. Fields not given are empty. Field values are HTML.", note_add),
    command!("note.edit", Effect::Write, r#"{"id":N,"fields":{"Back":"html"}?,"tags":["replaces","all"]?}"#,
        "Change the given fields of a note. Scheduling is untouched.", note_edit),
    command!("note.delete", Destructive, IDS, "Delete notes, with their cards and review history.", note_delete),
    command!("media.add", Effect::Write, r#"{"path":"file","name":"stored name"?}"#,
        "Copy a file into media. If that name is taken by different content, it's stored under a new name: use the returned name in fields.", media_add),
    command!("media.replace", Destructive, r#"{"path":"file","name":"stored name"}"#,
        "Overwrite a media file, keeping its name. The old one goes in backups/media-replaced. Browsers may show the old one for up to an hour.", media_replace),
    command!("card.suspend", Effect::Write, IDS, "Suspend cards.", card_suspend),
    command!("card.unsuspend", Effect::Write, IDS, "Unsuspend the suspended ones among these cards.", card_unsuspend),
    command!("card.set-due", Effect::Write, r#"{"ids":[]|"search":"","days":"0"|"1-7"|"3!"}"#,
        "Make cards due in this many days, from today. With !, the interval changes to match.", card_set_due),
    command!("tag.list", Read, "{}", "All tags.", tag_list),
    command!("tag.add", Effect::Write, r#"{"ids":[note ids]|"search":"","tags":["t"]}"#, "Add tags to notes.", tag_add),
    command!("tag.remove", Effect::Write, r#"{"ids":[note ids]|"search":"","tags":["t"]}"#, "Remove tags from notes.", tag_remove),
    command!("tag.rename", Destructive, r#"{"old":"a::b","new":"c"}"#, "Rename a tag, and its children, on every note.", tag_rename),
    command!("deck.list", Read, "{}", "Every deck, with today's counts.", deck_list),
    command!("deck.create", Effect::Write, r#"{"name":"A::B"}"#, "Create a deck, and any parents it needs.", deck_create),
    command!("deck.rename", Effect::Write, r#"{"deck":"name or id","name":"new"}"#, "Rename a deck.", deck_rename),
    command!("deck.delete", Destructive, r#"{"deck":"name or id"}"#, "Delete a deck, its children, and all their cards. Filtered decks give their cards back instead.", deck_delete),
    command!("deck.move-cards", Effect::Write, r#"{"ids":[]|"search":"","deck":"name or id"}"#, "Move cards to a deck.", deck_move_cards),
    command!("filtered.create", Effect::Write,
        r#"{"name":"x","search":"is:due","limit":100?,"order":"due"?,"reschedule":true?}"#,
        "Create a filtered deck and fill it. Orders: oldest_reviewed, random, intervals_ascending, intervals_descending, lapses, added, due, reverse_added, retrievability_ascending, retrievability_descending, relative_overdueness.", filtered_create),
    command!("filtered.rebuild", Effect::Write, r#"{"deck":"name or id"}"#, "Fill a filtered deck again.", filtered_rebuild),
    command!("filtered.empty", Effect::Write, r#"{"deck":"name or id"}"#, "Return a filtered deck's cards to their decks.", filtered_empty),
    command!("custom-study", Effect::Write,
        r#"{"deck":"name or id", and one of "new_limit_delta":N,"review_limit_delta":N,"forgot_days":N,"review_ahead_days":N,"preview_days":N,"cram":{"kind":"due|new|review|all","limit":N,"include":[],"exclude":[]}}"#,
        "As desktop's Custom Study. The last four replace the deck called Custom Study Session.", custom_study),
    command!("notetype.list", Read, "{}", "Note types and how many notes use them.", notetype_list),
    command!("notetype.show", Read, r#"{"notetype":"name or id"}"#, "A note type's fields, templates and CSS.", notetype_show),
    command!("notetype.add-field", Destructive, r#"{"notetype":"name or id","name":"Field"}"#, "Add a field at the end.", notetype_add_field),
    command!("notetype.edit-template", Destructive,
        r#"{"notetype":"name or id","template":"name or ord","question":"..."?,"answer":"..."?,"css":"..."?,"name":"new"?}"#,
        "Change a card template. Scripts in templates won't run.", notetype_edit_template),
    command!("check.database", Destructive, "{}", "Check the database and repair what it can. Returns what it found. Clears undo.", check_database),
    command!("check.media", Read, "{}", "Report media files that are missing or unused. Changes no cards; it can rename a badly-named file.", check_media),
    command!("cards.empty", Read, r#"{"apply":false?}"#,
        "Cards that have become blank on the front. With apply: true, deletes them.", cards_empty),
    command!("import", Destructive,
        r#"{"path":"x.apkg","with_scheduling":false?,"with_deck_configs":false?,"merge_notetypes":false?,"update_notes":"if_newer|always|never"?,"update_notetypes":"if_newer|always|never"?} or {"path":"x.colpkg","replace_collection":true}"#,
        "Import a package. A .colpkg replaces the whole collection, so requires replace_collection, and pages may see an error while it runs.", import),
    command!("import.text", Destructive,
        r#"{"path":"x.csv","notetype":"name or id"?,"deck":"name or id"?,"fields":{"Front":1,"Back":2}?,"delimiter":"tab|pipe|semicolon|colon|comma|space"?,"html":false?,"tags":["all rows"]?,"updated_tags":["rows that update a note"]?,"tags_column":3?,"duplicates":"update|keep|add"?,"match_scope":"notetype|notetype_and_deck"?}"#,
        "Import notes from a text file, one per line. What isn't given comes from the file's own #headers, or is worked out. duplicates says what to do with a row whose first field is already in a note.", import_text),
    command!("backup.now", Read, "{}", "Take a backup now.", backup_now),
    command!("stats.summary", Read, "{}", "Counts of everything, what's due today, and FSRS coverage.", stats_summary),
    command!("undo.status", Read, "{}", "What undo would do next.", undo_status),
    command!("undo", Effect::Write, "{}", "Undo the last operation. Only remembered while a server stays running.", undo),
];

pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

// JSON arguments
//////////////////////////////////////////////////

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{key} must be a string"))
}

fn nonempty_string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let value = string(args, key)?;
    if value.trim().is_empty() {
        bail!("{key} is empty");
    }
    Ok(value)
}

fn int(args: &Value, key: &str) -> Result<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .with_context(|| format!("{key} must be an integer"))
}

fn flag(args: &Value, key: &str) -> Result<bool> {
    match args.get(key) {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .with_context(|| format!("{key} must be true or false")),
    }
}

fn int_list(args: &Value, key: &str) -> Result<Vec<i64>> {
    args.get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("{key} must be a list of ids"))?
        .iter()
        .map(|id| id.as_i64().with_context(|| format!("{id} isn't an id")))
        .collect()
}

fn string_list(args: &Value, key: &str) -> Result<Vec<String>> {
    match args.get(key) {
        None => Ok(vec![]),
        Some(value) => value
            .as_array()
            .with_context(|| format!("{key} must be a list of strings"))?
            .iter()
            .map(|s| {
                s.as_str()
                    .map(str::to_string)
                    .with_context(|| format!("{s} isn't a string"))
            })
            .collect(),
    }
}

/// The ids an operation is on. An empty search would mean every card in the
/// collection, which nobody means by accident.
fn targets(admin: &Admin, args: &Value, search_method: &str) -> Result<Vec<i64>> {
    let ids = if args.get("ids").is_some() {
        if args.get("search").is_some() {
            bail!("give either ids or search, not both");
        }
        int_list(args, "ids")?
    } else if args.get("search").is_some() {
        let search = nonempty_string(args, "search")
            .context("a search that selects everything isn't accepted here")?;
        search_ids(&admin.backend, search_method, search)?
    } else {
        bail!("give ids or search");
    };
    if ids.is_empty() {
        bail!("nothing matched");
    }
    // the backend takes these into a table with a unique key
    let mut seen = std::collections::HashSet::new();
    Ok(ids.into_iter().filter(|id| seen.insert(*id)).collect())
}

fn card_targets(admin: &Admin, args: &Value) -> Result<Vec<i64>> {
    targets(admin, args, "searchCards")
}

fn note_targets(admin: &Admin, args: &Value) -> Result<Vec<i64>> {
    targets(admin, args, "searchNotes")
}

fn search_ids(backend: &Backend, method: &str, search: &str) -> Result<Vec<i64>> {
    let response: SearchResponse = call_typed(
        backend,
        method,
        &SearchRequest {
            search: search.to_string(),
            order: None,
        },
    )?;
    Ok(response.ids)
}

fn deck_id(admin: &Admin, args: &Value, key: &str) -> Result<i64> {
    match args.get(key) {
        Some(Value::Number(id)) => id.as_i64().context("deck id"),
        Some(Value::String(name)) => {
            let id: DeckId = call_typed(
                &admin.backend,
                "getDeckIdByName",
                &generic::String { val: name.clone() },
            )
            .map_err(|_| anyhow!("no deck called {name:?}"))?;
            Ok(id.did)
        }
        _ => bail!("{key} must be a deck's name or id"),
    }
}

fn notetype(admin: &Admin, args: &Value) -> Result<Notetype> {
    let ntid = match args.get("notetype") {
        Some(Value::Number(id)) => id.as_i64().context("notetype id")?,
        Some(Value::String(name)) => {
            let id: NotetypeId = call_typed(
                &admin.backend,
                "getNotetypeIdByName",
                &generic::String { val: name.clone() },
            )
            .map_err(|_| anyhow!("no note type called {name:?}"))?;
            id.ntid
        }
        _ => bail!("notetype must be a name or id"),
    };
    call_typed(&admin.backend, "getNotetype", &NotetypeId { ntid })
}

/// Other tools send a file's path to the server, which may be running in
/// another folder, so it's made absolute before being sent.
pub fn absolute_paths(args: &mut Value) -> Result<()> {
    if let Some(path) = args.get("path").and_then(Value::as_str) {
        args["path"] = utf8(&std::path::absolute(path)?)?.into();
    }
    Ok(())
}

// Search and show
//////////////////////////////////////////////////

fn search(admin: &Admin, args: &Value, method: &str) -> Result<Value> {
    let mut ids = search_ids(&admin.backend, method, string(args, "query")?)?;
    let count = ids.len();
    if args.get("limit").is_some() {
        ids.truncate(int(args, "limit")?.max(0) as usize);
    }
    Ok(json!({"count": count, "ids": ids}))
}

fn search_notes(admin: &Admin, args: &Value) -> Result<Value> {
    search(admin, args, "searchNotes")
}

fn search_cards(admin: &Admin, args: &Value) -> Result<Value> {
    search(admin, args, "searchCards")
}

fn card_json(card: &Card) -> Value {
    json!({
        "id": card.id,
        "note_id": card.note_id,
        "deck_id": card.deck_id,
        "template_ord": card.template_idx,
        "type": card.ctype,
        "queue": card.queue,
        "due": card.due,
        "interval": card.interval,
        "ease_factor": card.ease_factor,
        "reps": card.reps,
        "lapses": card.lapses,
        "original_due": card.original_due,
        "original_deck_id": card.original_deck_id,
        "stability": card.memory_state.as_ref().map(|m| m.stability),
        "difficulty": card.memory_state.as_ref().map(|m| m.difficulty),
        "desired_retention": card.desired_retention,
        "decay": card.decay,
    })
}

fn note_show(admin: &Admin, args: &Value) -> Result<Value> {
    let nid = int(args, "id")?;
    let note: Note = call_typed(&admin.backend, "getNote", &NoteId { nid })?;
    let nt: Notetype = call_typed(
        &admin.backend,
        "getNotetype",
        &NotetypeId {
            ntid: note.notetype_id,
        },
    )?;
    let fields: BTreeMap<_, _> = nt
        .fields
        .iter()
        .map(|f| &f.name)
        .zip(&note.fields)
        .collect();
    let cards: CardIds = call_typed(&admin.backend, "cardsOfNote", &NoteId { nid })?;
    let cards = cards
        .cids
        .iter()
        .map(|&cid| {
            let card: Card = call_typed(&admin.backend, "getCard", &CardId { cid })?;
            Ok(card_json(&card))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "id": note.id,
        "notetype": nt.name,
        "field_order": nt.fields.iter().map(|f| &f.name).collect::<Vec<_>>(),
        "fields": fields,
        "tags": note.tags,
        "cards": cards,
    }))
}

/// A rendered side, as the reviewer would show it before media and scripts.
pub fn flatten(nodes: &[RenderedTemplateNode]) -> String {
    nodes
        .iter()
        .map(|node| match &node.value {
            Some(rendered_template_node::Value::Text(text)) => text.as_str(),
            Some(rendered_template_node::Value::Replacement(replaced)) => {
                replaced.current_text.as_str()
            }
            None => "",
        })
        .collect()
}

fn card_show(admin: &Admin, args: &Value) -> Result<Value> {
    let cid = int(args, "id")?;
    let card: Card = call_typed(&admin.backend, "getCard", &CardId { cid })?;
    let stats: CardStatsResponse = call_typed(&admin.backend, "cardStats", &CardId { cid })?;
    let mut out = card_json(&card);
    out["deck"] = stats.deck.clone().into();
    out["notetype"] = stats.notetype.clone().into();
    out["retrievability"] = json!(stats.fsrs_retrievability);
    out["reviews"] = stats.revlog.len().into();
    if args.get("render").and_then(Value::as_bool).unwrap_or(true) {
        let rendered: RenderCardResponse = call_typed(
            &admin.backend,
            "renderExistingCard",
            &RenderExistingCardRequest {
                card_id: cid,
                browser: false,
                partial_render: false,
            },
        )?;
        out["question"] = flatten(&rendered.question_nodes).into();
        out["answer"] = flatten(&rendered.answer_nodes).into();
    }
    Ok(out)
}

// Notes
//////////////////////////////////////////////////

fn set_fields(note: &mut Note, nt: &Notetype, fields: &Value) -> Result<()> {
    let fields = fields.as_object().context("fields must be an object")?;
    for (name, value) in fields {
        let index = nt
            .fields
            .iter()
            .position(|f| &f.name == name)
            .with_context(|| {
                let known: Vec<_> = nt.fields.iter().map(|f| f.name.as_str()).collect();
                format!("{} has no field {name:?}, only {known:?}", nt.name)
            })?;
        note.fields[index] = value
            .as_str()
            .with_context(|| format!("field {name} must be a string"))?
            .to_string();
    }
    Ok(())
}

fn note_add(admin: &Admin, args: &Value) -> Result<Value> {
    let nt = notetype(admin, args)?;
    let deck_id = deck_id(admin, args, "deck")?;
    let mut note: Note = call_typed(&admin.backend, "newNote", &NotetypeId { ntid: nt.id })?;
    set_fields(
        &mut note,
        &nt,
        args.get("fields").context("fields are needed")?,
    )?;
    note.tags = string_list(args, "tags")?;

    use anki_proto::notes::note_fields_check_response::State;
    let check: NoteFieldsCheckResponse = call_typed(&admin.backend, "noteFieldsCheck", &note)?;
    match check.state() {
        State::Normal => {}
        State::Duplicate if flag(args, "allow_duplicate")? => {}
        State::Duplicate => {
            bail!("a note with this first field exists; pass allow_duplicate to add anyway")
        }
        State::Empty => bail!("the first field is empty"),
        State::MissingCloze => bail!("no cloze deletion in a cloze note"),
        State::NotetypeNotCloze => bail!("cloze deletion used in a note type that isn't cloze"),
        State::FieldNotCloze => bail!("cloze deletion in a field that doesn't take them"),
    }
    let added: AddNoteResponse = call_typed(
        &admin.backend,
        "addNote",
        &AddNoteRequest {
            note: Some(note),
            deck_id,
        },
    )?;
    let cards: CardIds = call_typed(
        &admin.backend,
        "cardsOfNote",
        &NoteId { nid: added.note_id },
    )?;
    Ok(json!({"id": added.note_id, "card_ids": cards.cids}))
}

fn note_edit(admin: &Admin, args: &Value) -> Result<Value> {
    let nid = int(args, "id")?;
    let mut note: Note = call_typed(&admin.backend, "getNote", &NoteId { nid })?;
    if args.get("fields").is_none() && args.get("tags").is_none() {
        bail!("give fields or tags");
    }
    if let Some(fields) = args.get("fields") {
        let nt: Notetype = call_typed(
            &admin.backend,
            "getNotetype",
            &NotetypeId {
                ntid: note.notetype_id,
            },
        )?;
        set_fields(&mut note, &nt, fields)?;
    }
    if args.get("tags").is_some() {
        note.tags = string_list(args, "tags")?;
    }
    let _: OpChanges = call_typed(
        &admin.backend,
        "updateNotes",
        &UpdateNotesRequest {
            notes: vec![note],
            skip_undo_entry: false,
        },
    )?;
    let cards: CardIds = call_typed(&admin.backend, "cardsOfNote", &NoteId { nid })?;
    Ok(json!({"id": nid, "card_ids": cards.cids}))
}

fn note_delete(admin: &Admin, args: &Value) -> Result<Value> {
    let note_ids = note_targets(admin, args)?;
    let removed: OpChangesWithCount = call_typed(
        &admin.backend,
        "removeNotes",
        &RemoveNotesRequest {
            note_ids,
            card_ids: vec![],
        },
    )?;
    Ok(json!({"deleted_notes": removed.count}))
}

// Media
//////////////////////////////////////////////////

fn media_add(admin: &Admin, args: &Value) -> Result<Value> {
    let path = Path::new(string(args, "path")?);
    let data = fs::read(path).with_context(|| path.display().to_string())?;
    let wanted = match args.get("name") {
        Some(_) => string(args, "name")?.to_string(),
        None => path
            .file_name()
            .and_then(|name| name.to_str())
            .context("the path has no usable file name")?
            .to_string(),
    };
    let stored: generic::String = call_typed(
        &admin.backend,
        "addMediaFile",
        &AddMediaFileRequest {
            desired_name: wanted.clone(),
            data,
        },
    )?;
    Ok(json!({
        "name": stored.val,
        "renamed": stored.val != wanted,
        "sound_tag": format!("[sound:{}]", stored.val),
        "image_tag": format!("<img src=\"{}\">", stored.val),
    }))
}

fn media_replace(admin: &Admin, args: &Value) -> Result<Value> {
    let path = Path::new(string(args, "path")?);
    let name = string(args, "name")?;
    if filename_if_normalized(name).as_deref() != Some(name) {
        bail!("{name:?} isn't a valid media file name");
    }
    let data = fs::read(path).with_context(|| path.display().to_string())?;
    let target = admin.media_dir().join(name);
    let keep_folder = admin.backups.folder().join("media-replaced");
    // holding the collection, so that a media check can't see it half done
    let previous = admin.backend.with_collection(|_col| {
        Ok((|| -> Result<Option<String>> {
            let mut previous = None;
            if target.exists() {
                fs::create_dir_all(&keep_folder)?;
                let keep = keep_folder.join(format!(
                    "{}-{name}",
                    Local::now().format("%Y-%m-%d-%H.%M.%S%.3f")
                ));
                fs::copy(&target, &keep)?;
                previous = Some(utf8(&keep)?);
            }
            // beside the folder rather than in it, where it could be taken for media
            let temporary = admin
                .media_dir()
                .with_extension(format!("media-replace-{}", std::process::id()));
            fs::write(&temporary, &data)?;
            fs::File::open(&temporary)?.sync_all()?;
            if let Err(err) = fs::rename(&temporary, &target) {
                let _ = fs::remove_file(&temporary);
                return Err(err.into());
            }
            Ok(previous)
        })())
    })??;
    Ok(json!({"name": name, "previous_kept_at": previous}))
}

// Scheduling
//////////////////////////////////////////////////

fn card_suspend(admin: &Admin, args: &Value) -> Result<Value> {
    let card_ids = card_targets(admin, args)?;
    let changed: OpChangesWithCount = call_typed(
        &admin.backend,
        "suspendCards",
        &SuspendCardsRequest {
            card_ids,
            note_ids: vec![],
        },
    )?;
    Ok(json!({"changed": changed.count}))
}

fn card_unsuspend(admin: &Admin, args: &Value) -> Result<Value> {
    let wanted = card_targets(admin, args)?;
    let suspended: std::collections::HashSet<i64> =
        search_ids(&admin.backend, "searchCards", "is:suspended")?
            .into_iter()
            .collect();
    let cids: Vec<i64> = wanted
        .into_iter()
        .filter(|id| suspended.contains(id))
        .collect();
    let changed = cids.len();
    if changed > 0 {
        let _: OpChanges = call_typed(&admin.backend, "unsuspendCards", &CardIds { cids })?;
    }
    Ok(json!({"changed": changed}))
}

fn card_set_due(admin: &Admin, args: &Value) -> Result<Value> {
    let card_ids = card_targets(admin, args)?;
    let count = card_ids.len();
    let _: OpChanges = call_typed(
        &admin.backend,
        "setDueDate",
        &SetDueDateRequest {
            card_ids,
            days: string(args, "days")?.to_string(),
            config_key: None,
        },
    )?;
    Ok(json!({"changed": count}))
}

// Tags
//////////////////////////////////////////////////

fn tag_list(admin: &Admin, _args: &Value) -> Result<Value> {
    let tags: generic::StringList = call_typed(&admin.backend, "allTags", &generic::Empty {})?;
    Ok(json!({"tags": tags.vals}))
}

fn change_tags(admin: &Admin, args: &Value, method: &str) -> Result<Value> {
    let note_ids = note_targets(admin, args)?;
    let tags = string_list(args, "tags")?;
    if tags.is_empty() {
        bail!("tags are needed");
    }
    let changed: OpChangesWithCount = call_typed(
        &admin.backend,
        method,
        &NoteIdsAndTagsRequest {
            note_ids,
            tags: tags.join(" "),
        },
    )?;
    Ok(json!({"changed_notes": changed.count}))
}

fn tag_add(admin: &Admin, args: &Value) -> Result<Value> {
    change_tags(admin, args, "addNoteTags")
}

fn tag_remove(admin: &Admin, args: &Value) -> Result<Value> {
    change_tags(admin, args, "removeNoteTags")
}

fn tag_rename(admin: &Admin, args: &Value) -> Result<Value> {
    let changed: OpChangesWithCount = call_typed(
        &admin.backend,
        "renameTags",
        &RenameTagsRequest {
            current_prefix: nonempty_string(args, "old")?.to_string(),
            new_prefix: nonempty_string(args, "new")?.to_string(),
        },
    )?;
    Ok(json!({"changed_notes": changed.count}))
}

// Decks
//////////////////////////////////////////////////

fn deck_rows(node: &DeckTreeNode, parent: &str, out: &mut Vec<Value>) {
    let name = if parent.is_empty() {
        node.name.clone()
    } else {
        format!("{parent}::{}", node.name)
    };
    // the root is nameless, and holds no cards itself
    if node.deck_id != 0 {
        out.push(json!({
            "id": node.deck_id,
            "name": name,
            "filtered": node.filtered,
            "new": node.new_count,
            "learn": node.learn_count,
            "review": node.review_count,
            "cards_in_deck": node.total_in_deck,
            "cards_with_children": node.total_including_children,
        }));
    }
    for child in &node.children {
        deck_rows(child, &name, out);
    }
}

fn now_secs() -> i64 {
    anki::timestamp::TimestampSecs::now().0
}

/// Every deck, and today's counts after limits.
pub fn deck_rows_today(backend: &Backend) -> Result<Vec<Value>> {
    let tree: DeckTreeNode = call_typed(backend, "deckTree", &DeckTreeRequest { now: now_secs() })?;
    let mut rows = vec![];
    deck_rows(&tree, "", &mut rows);
    Ok(rows)
}

fn deck_list(admin: &Admin, _args: &Value) -> Result<Value> {
    Ok(json!({"decks": deck_rows_today(&admin.backend)?}))
}

fn deck_exists(admin: &Admin, name: &str) -> bool {
    let id: Result<DeckId> = call_typed(
        &admin.backend,
        "getDeckIdByName",
        &generic::String {
            val: name.to_string(),
        },
    );
    id.is_ok()
}

fn deck_create(admin: &Admin, args: &Value) -> Result<Value> {
    let name = nonempty_string(args, "name")?;
    if deck_exists(admin, name) {
        bail!("there's already a deck called {name:?}");
    }
    let mut deck: Deck = call_typed(&admin.backend, "newDeck", &generic::Empty {})?;
    deck.name = name.to_string();
    let added: OpChangesWithId = call_typed(&admin.backend, "addDeck", &deck)?;
    Ok(json!({"id": added.id}))
}

fn deck_rename(admin: &Admin, args: &Value) -> Result<Value> {
    let deck_id = deck_id(admin, args, "deck")?;
    let new_name = nonempty_string(args, "name")?.to_string();
    if deck_exists(admin, &new_name) {
        bail!("there's already a deck called {new_name:?}");
    }
    let _: OpChanges = call_typed(
        &admin.backend,
        "renameDeck",
        &RenameDeckRequest { deck_id, new_name },
    )?;
    Ok(json!({"id": deck_id}))
}

fn deck_delete(admin: &Admin, args: &Value) -> Result<Value> {
    let did = deck_id(admin, args, "deck")?;
    let removed: OpChangesWithCount =
        call_typed(&admin.backend, "removeDecks", &DeckIds { dids: vec![did] })?;
    Ok(json!({"deleted_cards": removed.count}))
}

fn deck_move_cards(admin: &Admin, args: &Value) -> Result<Value> {
    let card_ids = card_targets(admin, args)?;
    let deck_id = deck_id(admin, args, "deck")?;
    let changed: OpChangesWithCount = call_typed(
        &admin.backend,
        "setDeck",
        &SetDeckRequest { card_ids, deck_id },
    )?;
    Ok(json!({"moved": changed.count}))
}

// Filtered decks and custom study
//////////////////////////////////////////////////

fn order(name: &str) -> Result<Order> {
    Ok(match name {
        "oldest_reviewed" => Order::OldestReviewedFirst,
        "random" => Order::Random,
        "intervals_ascending" => Order::IntervalsAscending,
        "intervals_descending" => Order::IntervalsDescending,
        "lapses" => Order::Lapses,
        "added" => Order::Added,
        "due" => Order::Due,
        "reverse_added" => Order::ReverseAdded,
        "retrievability_ascending" => Order::RetrievabilityAscending,
        "retrievability_descending" => Order::RetrievabilityDescending,
        "relative_overdueness" => Order::RelativeOverdueness,
        _ => bail!("unknown order {name:?}"),
    })
}

fn filtered_create(admin: &Admin, args: &Value) -> Result<Value> {
    let name = nonempty_string(args, "name")?.to_string();
    if deck_exists(admin, &name) {
        bail!("there's already a deck called {name:?}");
    }
    let mut update: FilteredDeckForUpdate = call_typed(
        &admin.backend,
        "getOrCreateFilteredDeck",
        &DeckId { did: 0 },
    )?;
    update.name = name;
    let config = update.config.as_mut().context("no filtered config")?;
    config.reschedule = args
        .get("reschedule")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100);
    let order_name = args.get("order").and_then(Value::as_str).unwrap_or("due");
    config.search_terms = vec![SearchTerm {
        search: nonempty_string(args, "search")?.to_string(),
        limit: limit as u32,
        order: order(order_name)? as i32,
    }];
    update.allow_empty = false;
    let added: OpChangesWithId = call_typed(&admin.backend, "addOrUpdateFilteredDeck", &update)?;
    let cards = search_ids(&admin.backend, "searchCards", &format!("did:{}", added.id))?;
    Ok(json!({"id": added.id, "cards": cards.len()}))
}

fn filtered_deck(admin: &Admin, args: &Value) -> Result<i64> {
    let did = deck_id(admin, args, "deck")?;
    let deck: Deck = call_typed(&admin.backend, "getDeck", &DeckId { did })?;
    if !matches!(deck.kind, Some(anki_proto::decks::deck::Kind::Filtered(_))) {
        bail!("{:?} isn't a filtered deck", deck.name);
    }
    Ok(did)
}

fn filtered_rebuild(admin: &Admin, args: &Value) -> Result<Value> {
    let did = filtered_deck(admin, args)?;
    let rebuilt: OpChangesWithCount =
        call_typed(&admin.backend, "rebuildFilteredDeck", &DeckId { did })?;
    Ok(json!({"cards": rebuilt.count}))
}

fn filtered_empty(admin: &Admin, args: &Value) -> Result<Value> {
    let did = filtered_deck(admin, args)?;
    let _: OpChanges = call_typed(&admin.backend, "emptyFilteredDeck", &DeckId { did })?;
    Ok(json!({"id": did}))
}

fn custom_study(admin: &Admin, args: &Value) -> Result<Value> {
    let deck_id = deck_id(admin, args, "deck")?;
    let number = |key: &str| args.get(key).and_then(Value::as_i64);
    let value = if let Some(n) = number("new_limit_delta") {
        StudyValue::NewLimitDelta(n as i32)
    } else if let Some(n) = number("review_limit_delta") {
        StudyValue::ReviewLimitDelta(n as i32)
    } else if let Some(n) = number("forgot_days") {
        StudyValue::ForgotDays(n as u32)
    } else if let Some(n) = number("review_ahead_days") {
        StudyValue::ReviewAheadDays(n as u32)
    } else if let Some(n) = number("preview_days") {
        StudyValue::PreviewDays(n as u32)
    } else if let Some(cram) = args.get("cram") {
        let kind = match string(cram, "kind")? {
            "due" => CramKind::Due,
            "new" => CramKind::New,
            "review" => CramKind::Review,
            "all" => CramKind::All,
            other => bail!("unknown cram kind {other:?}"),
        };
        StudyValue::Cram(Cram {
            kind: kind as i32,
            card_limit: cram.get("limit").and_then(Value::as_u64).unwrap_or(100) as u32,
            tags_to_include: string_list(cram, "include")?,
            tags_to_exclude: string_list(cram, "exclude")?,
        })
    } else {
        bail!("say what to study; `evolanki-admin commands` has the options");
    };
    let _: OpChanges = call_typed(
        &admin.backend,
        "customStudy",
        &CustomStudyRequest {
            deck_id,
            value: Some(value),
        },
    )?;
    Ok(json!({"done": true}))
}

// Note types
//////////////////////////////////////////////////

fn notetype_list(admin: &Admin, _args: &Value) -> Result<Value> {
    let counts: NotetypeUseCounts = call_typed(
        &admin.backend,
        "getNotetypeNamesAndCounts",
        &generic::Empty {},
    )?;
    let rows: Vec<_> = counts
        .entries
        .iter()
        .map(|e| json!({"id": e.id, "name": e.name, "notes": e.use_count}))
        .collect();
    Ok(json!({"notetypes": rows}))
}

fn notetype_show(admin: &Admin, args: &Value) -> Result<Value> {
    let nt = notetype(admin, args)?;
    let config = nt.config.clone().unwrap_or_default();
    let templates: Vec<_> = nt
        .templates
        .iter()
        .map(|t| {
            let c = t.config.clone().unwrap_or_default();
            json!({
                "ord": t.ord.as_ref().map(|o| o.val),
                "name": t.name,
                "question": c.q_format,
                "answer": c.a_format,
            })
        })
        .collect();
    let fields: Vec<_> = nt.fields.iter().map(|f| &f.name).collect();
    Ok(json!({
        "id": nt.id,
        "name": nt.name,
        "cloze": config.kind == notetype::config::Kind::Cloze as i32,
        "fields": fields,
        "templates": templates,
        "css": config.css,
    }))
}

fn update_notetype(admin: &Admin, nt: &Notetype) -> Result<()> {
    let _: OpChanges = call_typed(&admin.backend, "updateNotetype", nt)?;
    Ok(())
}

fn notetype_add_field(admin: &Admin, args: &Value) -> Result<Value> {
    let mut nt = notetype(admin, args)?;
    let name = nonempty_string(args, "name")?.to_string();
    if nt.fields.iter().any(|f| f.name == name) {
        bail!("{} already has a field called {name:?}", nt.name);
    }
    nt.fields.push(notetype::Field {
        ord: None,
        name,
        config: Some(notetype::field::Config {
            font_name: "Arial".to_string(),
            font_size: 20,
            ..Default::default()
        }),
    });
    update_notetype(admin, &nt)?;
    Ok(json!({"fields": nt.fields.iter().map(|f| &f.name).collect::<Vec<_>>()}))
}

fn notetype_edit_template(admin: &Admin, args: &Value) -> Result<Value> {
    let mut nt = notetype(admin, args)?;
    let wanted = args.get("template").context("template is needed")?;
    let template = nt
        .templates
        .iter_mut()
        .find(|t| match wanted {
            Value::String(name) => &t.name == name,
            other => t.ord.as_ref().map(|o| o.val as u64) == other.as_u64(),
        })
        .with_context(|| format!("no template {wanted}"))?;
    let config = template.config.get_or_insert_with(Default::default);
    let mut changes = 0;
    if args.get("question").is_some() {
        config.q_format = string(args, "question")?.to_string();
        changes += 1;
    }
    if args.get("answer").is_some() {
        config.a_format = string(args, "answer")?.to_string();
        changes += 1;
    }
    if args.get("name").is_some() {
        template.name = nonempty_string(args, "name")?.to_string();
        changes += 1;
    }
    if args.get("css").is_some() {
        nt.config.get_or_insert_with(Default::default).css = string(args, "css")?.to_string();
        changes += 1;
    }
    if changes == 0 {
        bail!("give question, answer, css or name");
    }
    update_notetype(admin, &nt)?;
    Ok(json!({"id": nt.id}))
}

// Checks
//////////////////////////////////////////////////

fn check_database(admin: &Admin, _args: &Value) -> Result<Value> {
    let checked: CheckDatabaseResponse =
        call_typed(&admin.backend, "checkDatabase", &generic::Empty {})?;
    Ok(json!({"problems": checked.problems}))
}

fn check_media(admin: &Admin, _args: &Value) -> Result<Value> {
    let checked: CheckMediaResponse = call_typed(&admin.backend, "checkMedia", &generic::Empty {})?;
    Ok(json!({
        "missing": checked.missing,
        "missing_media_notes": checked.missing_media_notes,
        "unused": checked.unused,
        "report": checked.report,
    }))
}

fn cards_empty(admin: &Admin, args: &Value) -> Result<Value> {
    let report: EmptyCardsReport = call_typed(&admin.backend, "getEmptyCards", &generic::Empty {})?;
    let card_ids: Vec<i64> = report
        .notes
        .iter()
        .flat_map(|note| note.card_ids.iter().copied())
        .collect();
    let note_ids: Vec<i64> = report.notes.iter().map(|note| note.note_id).collect();
    let mut deleted = 0;
    let mut backup = Value::Null;
    if flag(args, "apply")? && !card_ids.is_empty() {
        backup = utf8(&admin.backups.try_take(&admin.backend)?)?.into();
        let removed: OpChangesWithCount = call_typed(
            &admin.backend,
            "removeCards",
            &RemoveCardsRequest {
                card_ids: card_ids.clone(),
            },
        )?;
        deleted = removed.count;
    }
    Ok(json!({
        "empty_cards": card_ids,
        "notes": note_ids,
        "deleted": deleted,
        "backup": backup,
    }))
}

fn backup_now(admin: &Admin, _args: &Value) -> Result<Value> {
    Ok(json!({"path": utf8(&admin.backups.try_take(&admin.backend)?)?}))
}

// Import
//////////////////////////////////////////////////

fn update_condition(args: &Value, key: &str) -> Result<i32> {
    Ok(match args.get(key).and_then(Value::as_str) {
        None | Some("if_newer") => ImportAnkiPackageUpdateCondition::IfNewer,
        Some("always") => ImportAnkiPackageUpdateCondition::Always,
        Some("never") => ImportAnkiPackageUpdateCondition::Never,
        Some(other) => bail!("{key}: unknown value {other:?}"),
    } as i32)
}

fn import(admin: &Admin, args: &Value) -> Result<Value> {
    let path = string(args, "path")?;
    if !Path::new(path).is_file() {
        bail!("{path} isn't a file");
    }
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("apkg") => import_apkg(admin, path, args),
        Some("colpkg") => import_colpkg(admin, path, args),
        _ => bail!("only .apkg and .colpkg can be imported here; import.text takes text files"),
    }
}

fn import_apkg(admin: &Admin, path: &str, args: &Value) -> Result<Value> {
    let imported: ImportResponse = call_typed(
        &admin.backend,
        "importAnkiPackage",
        &ImportAnkiPackageRequest {
            package_path: path.to_string(),
            options: Some(ImportAnkiPackageOptions {
                merge_notetypes: flag(args, "merge_notetypes")?,
                update_notes: update_condition(args, "update_notes")?,
                update_notetypes: update_condition(args, "update_notetypes")?,
                with_scheduling: flag(args, "with_scheduling")?,
                with_deck_configs: flag(args, "with_deck_configs")?,
            }),
        },
    )?;
    let filled = fill_missing_memory_states(&admin.backend)?;
    let mut result = import_log(imported);
    result["memory_states_added"] = filled.into();
    Ok(result)
}

fn import_log(imported: ImportResponse) -> Value {
    let log = imported.log.unwrap_or_default();
    json!({
        "new": log.new.len(),
        "updated": log.updated.len(),
        "duplicate": log.duplicate.len(),
        "conflicting": log.conflicting.len(),
        "first_field_match": log.first_field_match.len(),
        "missing_notetype": log.missing_notetype.len(),
        "missing_deck": log.missing_deck.len(),
        "empty_first_field": log.empty_first_field.len(),
        "found_notes": log.found_notes,
    })
}

fn csv_delimiter(name: &str) -> Result<csv_metadata::Delimiter> {
    use csv_metadata::Delimiter::*;
    Ok(match name {
        "tab" => Tab,
        "pipe" => Pipe,
        "semicolon" => Semicolon,
        "colon" => Colon,
        "comma" => Comma,
        "space" => Space,
        other => bail!("delimiter: unknown value {other:?}"),
    })
}

fn csv_dupe_resolution(name: &str) -> Result<csv_metadata::DupeResolution> {
    use csv_metadata::DupeResolution::*;
    Ok(match name {
        "update" => Update,
        "keep" => Preserve,
        "add" => Duplicate,
        other => bail!("duplicates: unknown value {other:?}"),
    })
}

fn csv_match_scope(name: &str) -> Result<csv_metadata::MatchScope> {
    Ok(match name {
        "notetype" => csv_metadata::MatchScope::Notetype,
        "notetype_and_deck" => csv_metadata::MatchScope::NotetypeAndDeck,
        other => bail!("match_scope: unknown value {other:?}"),
    })
}

/// Which column each of a notetype's fields comes from, one-based.
fn csv_field_columns(nt: &Notetype, fields: &Value) -> Result<Vec<u32>> {
    let fields = fields.as_object().context("fields must be an object")?;
    let mut columns = vec![0; nt.fields.len()];
    for (name, column) in fields {
        let idx = nt
            .fields
            .iter()
            .position(|field| field.name == *name)
            .with_context(|| format!("{} has no field {name:?}", nt.name))?;
        columns[idx] = column
            .as_u64()
            .filter(|column| *column > 0)
            .with_context(|| format!("the column of {name} must be a number from 1"))?
            as u32;
    }
    Ok(columns)
}

/// The backend works out what it can from the file, and what's given replaces that.
fn import_text(admin: &Admin, args: &Value) -> Result<Value> {
    let path = string(args, "path")?;
    if !Path::new(path).is_file() {
        bail!("{path} isn't a file");
    }
    let given = |key: &str| args.get(key).filter(|value| !value.is_null());
    let notetype = if given("notetype").is_some() {
        Some(notetype(admin, args)?)
    } else {
        None
    };
    let deck = if given("deck").is_some() {
        Some(deck_id(admin, args, "deck")?)
    } else {
        None
    };
    let delimiter = if given("delimiter").is_some() {
        Some(csv_delimiter(string(args, "delimiter")?)? as i32)
    } else {
        None
    };
    let mut metadata: CsvMetadata = call_typed(
        &admin.backend,
        "getCsvMetadata",
        &CsvMetadataRequest {
            path: path.to_string(),
            delimiter,
            notetype_id: notetype.as_ref().map(|nt| nt.id),
            deck_id: deck,
            is_html: if given("html").is_some() {
                Some(flag(args, "html")?)
            } else {
                None
            },
        },
    )?;
    if let Some(fields) = given("fields") {
        let Some(nt) = &notetype else {
            bail!("fields needs notetype, to say which fields those are");
        };
        metadata.notetype = Some(csv_metadata::Notetype::GlobalNotetype(
            csv_metadata::MappedNotetype {
                id: nt.id,
                field_columns: csv_field_columns(nt, fields)?,
            },
        ));
    }
    if given("tags").is_some() {
        metadata.global_tags = string_list(args, "tags")?;
    }
    if given("updated_tags").is_some() {
        metadata.updated_tags = string_list(args, "updated_tags")?;
    }
    if given("tags_column").is_some() {
        metadata.tags_column = u32::try_from(int(args, "tags_column")?)?;
    }
    if given("duplicates").is_some() {
        metadata.dupe_resolution = csv_dupe_resolution(string(args, "duplicates")?)? as i32;
    }
    if given("match_scope").is_some() {
        metadata.match_scope = csv_match_scope(string(args, "match_scope")?)? as i32;
    }
    let imported: ImportResponse = call_typed(
        &admin.backend,
        "importCsv",
        &ImportCsvRequest {
            path: path.to_string(),
            metadata: Some(metadata),
        },
    )?;
    Ok(import_log(imported))
}

/// A package can bring reviewed cards that have no memory state. Returns how
/// many there were. Only clears undo if there were any.
pub fn fill_missing_memory_states(backend: &Backend) -> Result<u32> {
    Ok(backend.with_collection(|col| {
        let missing = col.count_missing_memory_states()?;
        if missing > 0 {
            col.add_missing_fsrs_memory_states()?;
        }
        Ok(missing)
    })?)
}

/// This replaces the file under the backend, so it needs the collection closed.
/// Whatever happens, it's opened again, and pages see an error only meanwhile.
fn import_colpkg(admin: &Admin, path: &str, args: &Value) -> Result<Value> {
    if !flag(args, "replace_collection")? {
        bail!("a .colpkg replaces the whole collection; pass replace_collection to go ahead");
    }
    let collection = admin.collection().canonicalize()?;
    let (media_folder, media_db) = media_paths(&collection);
    close_collection(&admin.backend)?;
    let imported: Result<generic::Empty> = call_typed(
        &admin.backend,
        "importCollectionPackage",
        &ImportCollectionPackageRequest {
            col_path: utf8(&collection)?,
            backup_path: path.to_string(),
            media_folder: utf8(&media_folder)?,
            media_db: utf8(&media_db)?,
        },
    );
    open_collection(&admin.backend, &collection)
        .context("the collection couldn't be opened after importing")?;
    imported?;
    let filled = fill_missing_memory_states(&admin.backend)?;
    Ok(json!({"replaced": true, "memory_states_added": filled}))
}

// Summary and undo
//////////////////////////////////////////////////

fn stats_summary(admin: &Admin, _args: &Value) -> Result<Value> {
    let timing: SchedTimingTodayResponse =
        call_typed(&admin.backend, "schedTimingToday", &generic::Empty {})?;
    let today = timing.days_elapsed;
    let studied: generic::String = call_typed(&admin.backend, "studiedToday", &generic::Empty {})?;
    let mut out = admin
        .backend
        .with_collection(|col| {
            let db = col.storage.db();
            let count = |sql: &str| -> anki::error::Result<i64> {
                Ok(db.query_row(sql, [], |row| row.get(0))?)
            };
            Ok(json!({
                "notes": count("select count() from notes")?,
                "cards": count("select count() from cards")?,
                "revlog": count("select count() from revlog")?,
                "decks": count("select count() from decks")?,
                "notetypes": count("select count() from notetypes")?,
                "new": count("select count() from cards where queue = 0")?,
                "learning": count("select count() from cards where queue in (1, 3)")?,
                "review": count("select count() from cards where queue = 2")?,
                "suspended": count("select count() from cards where queue = -1")?,
                "with_memory_state": count(
                    "select count() from cards where data like '%\"s\":%'"
                )?,
                "reviews_due_today": db.query_row(
                    "select count() from cards where queue = 2 and due <= ?",
                    [today],
                    |row| row.get::<_, i64>(0),
                )?,
            }))
        })
        .map_err(|err| anyhow!("{err}"))?;
    out["days_elapsed"] = today.into();
    out["studied_today"] = studied.val.into();
    let mut rows = deck_rows_today(&admin.backend)?;
    rows.retain(|row| !row["name"].as_str().unwrap_or_default().contains("::"));
    out["due_after_limits"] = json!({
        "new": rows.iter().filter_map(|r| r["new"].as_u64()).sum::<u64>(),
        "learn": rows.iter().filter_map(|r| r["learn"].as_u64()).sum::<u64>(),
        "review": rows.iter().filter_map(|r| r["review"].as_u64()).sum::<u64>(),
    });
    Ok(out)
}

fn undo_status(admin: &Admin, _args: &Value) -> Result<Value> {
    let status: UndoStatus = call_typed(&admin.backend, "getUndoStatus", &generic::Empty {})?;
    Ok(json!({"undo": status.undo, "redo": status.redo}))
}

fn undo(admin: &Admin, _args: &Value) -> Result<Value> {
    let status: UndoStatus = call_typed(&admin.backend, "getUndoStatus", &generic::Empty {})?;
    if status.undo.is_empty() {
        bail!("nothing to undo");
    }
    let _: anki_proto::collection::OpChangesAfterUndo =
        call_typed(&admin.backend, "undo", &generic::Empty {})?;
    Ok(json!({"undid": status.undo}))
}
