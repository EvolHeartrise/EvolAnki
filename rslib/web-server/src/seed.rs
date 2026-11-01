// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Fills a new collection with notes and a few reviews, for development and
//! tests. Goes through the same RPCs as the pages do.

use std::path::Path;

use anki::backend::Backend;
use anki_proto::cards::CardIds;
use anki_proto::collection::OpChangesWithId;
use anki_proto::decks::deck::Kind;
use anki_proto::decks::Deck;
use anki_proto::generic;
use anki_proto::notes::AddNoteRequest;
use anki_proto::notes::AddNoteResponse;
use anki_proto::notes::Note;
use anki_proto::notes::NoteId;
use anki_proto::notetypes::notetype;
use anki_proto::notetypes::Notetype;
use anki_proto::notetypes::NotetypeId;
use anki_proto::notetypes::NotetypeNames;
use anki_proto::scheduler::card_answer::Rating;
use anki_proto::scheduler::CardAnswer;
use anki_proto::scheduler::GetQueuedCardsRequest;
use anki_proto::scheduler::QueuedCards;
use anki_proto::scheduler::SetDueDateRequest;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;

use crate::rpc::call_typed;

const DEFAULT_DECK: i64 = 1;
const NOTES: usize = 20;
const REVIEWS: usize = 5;

pub fn seed_demo_content(backend: &Backend) -> Result<()> {
    let names: NotetypeNames = call_typed(backend, "getNotetypeNames", &generic::Empty {})?;
    let basic = names
        .entries
        .iter()
        .find(|e| e.name == "Basic")
        .context("no Basic notetype")?;
    for i in 1..=NOTES {
        let mut note: Note = call_typed(backend, "newNote", &NotetypeId { ntid: basic.id })?;
        note.fields = vec![format!("Question {i}"), format!("Answer {i}")];
        let _: AddNoteResponse = call_typed(
            backend,
            "addNote",
            &AddNoteRequest {
                note: Some(note),
                deck_id: DEFAULT_DECK,
            },
        )?;
    }
    for _ in 0..REVIEWS {
        let queued: QueuedCards = call_typed(
            backend,
            "getQueuedCards",
            &GetQueuedCardsRequest {
                fetch_limit: 1,
                intraday_learning_only: false,
            },
        )?;
        let card = queued.cards.into_iter().next().context("no queued card")?;
        let states = card.states.context("no states")?;
        let answer = CardAnswer {
            card_id: card.card.context("no card")?.id,
            current_state: states.current,
            new_state: states.good,
            rating: Rating::Good as i32,
            answered_at_millis: anki::timestamp::TimestampMillis::now().0,
            milliseconds_taken: 1000,
        };
        let _: anki_proto::collection::OpChanges = call_typed(backend, "answerCard", &answer)?;
    }
    Ok(())
}

/// What `--seed-file` reads. Tests use it for content the API can't add.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    #[serde(default)]
    decks: Vec<SeedDeck>,
    #[serde(default)]
    notes: Vec<SeedNote>,
    #[serde(default)]
    notetypes: Vec<SeedNotetype>,
}

/// One card template, based on the Basic notetype.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedNotetype {
    name: String,
    fields: Vec<String>,
    front: String,
    back: String,
    /// Store this raw value, for a notetype kind that this version doesn't know.
    original_stock_kind: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedDeck {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    markdown: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedNote {
    /// Name of a deck listed above; the default deck if not given.
    deck: Option<String>,
    notetype: String,
    fields: Vec<String>,
    /// Make its cards reviews due today, rather than new.
    #[serde(default)]
    due: bool,
}

pub fn seed_from_file(backend: &Backend, path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let seed: SeedFile = serde_json::from_str(&text).context("invalid seed file")?;

    let mut decks = vec![];
    for deck in seed.decks {
        let mut new: Deck = call_typed(backend, "newDeck", &generic::Empty {})?;
        new.name = deck.name.clone();
        if let Some(Kind::Normal(normal)) = &mut new.kind {
            normal.description = deck.description;
            normal.markdown_description = deck.markdown;
        }
        let added: OpChangesWithId = call_typed(backend, "addDeck", &new)?;
        decks.push((deck.name, added.id));
    }

    let names: NotetypeNames = call_typed(backend, "getNotetypeNames", &generic::Empty {})?;
    let basic = names
        .entries
        .iter()
        .find(|e| e.name == "Basic")
        .context("no Basic notetype")?
        .id;
    for seed_notetype in seed.notetypes {
        add_notetype(backend, basic, seed_notetype)?;
    }
    let names: NotetypeNames = call_typed(backend, "getNotetypeNames", &generic::Empty {})?;
    for seed_note in seed.notes {
        let notetype = names
            .entries
            .iter()
            .find(|e| e.name == seed_note.notetype)
            .with_context(|| format!("no notetype {}", seed_note.notetype))?;
        let deck_id = match &seed_note.deck {
            Some(name) => {
                decks
                    .iter()
                    .find(|(deck, _)| deck == name)
                    .with_context(|| format!("no deck {name}"))?
                    .1
            }
            None => DEFAULT_DECK,
        };
        let mut note: Note = call_typed(backend, "newNote", &NotetypeId { ntid: notetype.id })?;
        note.fields = seed_note.fields;
        let added: AddNoteResponse = call_typed(
            backend,
            "addNote",
            &AddNoteRequest {
                note: Some(note),
                deck_id,
            },
        )?;
        if seed_note.due {
            let cards: CardIds =
                call_typed(backend, "cardsOfNote", &NoteId { nid: added.note_id })?;
            let _: anki_proto::collection::OpChanges = call_typed(
                backend,
                "setDueDate",
                &SetDueDateRequest {
                    card_ids: cards.cids,
                    days: "0".into(),
                    config_key: None,
                },
            )?;
        }
    }
    Ok(())
}

fn add_notetype(backend: &Backend, basic: i64, seed: SeedNotetype) -> Result<()> {
    let mut notetype: Notetype = call_typed(backend, "getNotetype", &NotetypeId { ntid: basic })?;
    notetype.id = 0;
    notetype.name = seed.name;
    let field = notetype.fields[0].clone();
    notetype.fields = seed
        .fields
        .into_iter()
        .map(|name| notetype::Field {
            ord: None,
            name,
            ..field.clone()
        })
        .collect();
    notetype.templates.truncate(1);
    let template = &mut notetype.templates[0];
    template.ord = None;
    template.id = None;
    let config = template.config.as_mut().context("no template config")?;
    config.q_format = seed.front;
    config.a_format = seed.back;
    if let Some(kind) = seed.original_stock_kind {
        notetype
            .config
            .as_mut()
            .context("no notetype config")?
            .original_stock_kind = kind;
    }
    let _: OpChangesWithId = call_typed(backend, "addNotetype", &notetype)?;
    Ok(())
}
