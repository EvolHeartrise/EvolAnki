// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Fills a new collection with notes and a few reviews, for development and
//! tests. Goes through the same RPCs as the pages do.

use anki::backend::Backend;
use anki_proto::generic;
use anki_proto::notes::AddNoteRequest;
use anki_proto::notes::AddNoteResponse;
use anki_proto::notes::Note;
use anki_proto::notetypes::NotetypeId;
use anki_proto::notetypes::NotetypeNames;
use anki_proto::scheduler::card_answer::Rating;
use anki_proto::scheduler::CardAnswer;
use anki_proto::scheduler::GetQueuedCardsRequest;
use anki_proto::scheduler::QueuedCards;
use anyhow::Context;
use anyhow::Result;

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
