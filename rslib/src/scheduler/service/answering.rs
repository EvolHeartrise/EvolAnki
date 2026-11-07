// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::prelude::*;
use crate::scheduler::answering::CardAnswer;
use crate::scheduler::answering::Rating;
use crate::scheduler::queue::QueuedCard;
use crate::scheduler::queue::QueuedCards;

impl From<anki_proto::scheduler::CardAnswer> for CardAnswer {
    fn from(answer: anki_proto::scheduler::CardAnswer) -> Self {
        CardAnswer {
            card_id: CardId(answer.card_id),
            rating: answer.rating().into(),
            answered_at: TimestampMillis(answer.answered_at_millis),
            milliseconds_taken: answer.milliseconds_taken,
            shown_as: Some((TimestampSecs(answer.card_mtime_secs), answer.card_reps)),
            from_queue: true,
        }
    }
}

impl From<anki_proto::scheduler::card_answer::Rating> for Rating {
    fn from(rating: anki_proto::scheduler::card_answer::Rating) -> Self {
        match rating {
            anki_proto::scheduler::card_answer::Rating::Again => Rating::Again,
            anki_proto::scheduler::card_answer::Rating::Hard => Rating::Hard,
            anki_proto::scheduler::card_answer::Rating::Good => Rating::Good,
            anki_proto::scheduler::card_answer::Rating::Easy => Rating::Easy,
        }
    }
}

impl From<QueuedCard> for anki_proto::scheduler::queued_cards::QueuedCard {
    fn from(queued_card: QueuedCard) -> Self {
        Self {
            card: Some(queued_card.card.into()),
            states: Some(queued_card.states.into()),
            context: Some(queued_card.context),
            labels: queued_card.labels,
            queue: match queued_card.kind {
                crate::scheduler::queue::QueueEntryKind::New => {
                    anki_proto::scheduler::queued_cards::Queue::New
                }
                crate::scheduler::queue::QueueEntryKind::Review => {
                    anki_proto::scheduler::queued_cards::Queue::Review
                }
                crate::scheduler::queue::QueueEntryKind::Learning => {
                    anki_proto::scheduler::queued_cards::Queue::Learning
                }
            } as i32,
        }
    }
}

impl From<QueuedCards> for anki_proto::scheduler::QueuedCards {
    fn from(queued_cards: QueuedCards) -> Self {
        Self {
            cards: queued_cards.cards.into_iter().map(Into::into).collect(),
            new_count: queued_cards.new_count as u32,
            learning_count: queued_cards.learning_count as u32,
            review_count: queued_cards.review_count as u32,
        }
    }
}
