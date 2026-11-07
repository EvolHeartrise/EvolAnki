// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;

use serde::Deserialize as DeTrait;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde_aux::field_attributes::deserialize_number_from_string;
use serde_json::Value;
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;
use serde_tuple::Serialize_tuple;

use super::DeckConfig;
use super::DeckConfigId;
use super::DeckConfigInner;
use super::NewCardInsertOrder;
use super::INITIAL_EASE_FACTOR_THOUSANDS;
use crate::notetype::schema11::other_to_bytes_without_usn;
use crate::serde::default_on_invalid;
use crate::timestamp::TimestampSecs;

fn wait_for_audio_default() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeckConfSchema11 {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub(crate) id: DeckConfigId,
    #[serde(rename = "mod", deserialize_with = "deserialize_number_from_string")]
    pub(crate) mtime: TimestampSecs,
    pub(crate) name: String,
    max_taken: i32,
    autoplay: bool,
    #[serde(deserialize_with = "default_on_invalid")]
    timer: u8,
    #[serde(default)]
    replayq: bool,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) new: NewConfSchema11,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) rev: RevConfSchema11,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) lapse: LapseConfSchema11,
    #[serde(rename = "dyn", default, deserialize_with = "default_on_invalid")]
    dynamic: bool,

    // 2021 scheduler options: these were not in schema 11, but we need to persist them
    // so the settings are not lost on upgrade/downgrade.
    #[serde(default)]
    new_mix: i32,
    #[serde(default)]
    new_per_day_minimum: u32,
    #[serde(default)]
    interday_learning_mix: i32,
    #[serde(default)]
    review_order: i32,
    #[serde(default)]
    new_sort_order: i32,
    #[serde(default)]
    new_gather_priority: i32,

    #[serde(default, rename = "fsrsWeights")]
    fsrs_params_4: Vec<f32>,
    #[serde(default)]
    fsrs_params_5: Vec<f32>,
    #[serde(default)]
    fsrs_params_6: Vec<f32>,
    #[serde(default)]
    desired_retention: f32,
    #[serde(default)]
    ignore_revlogs_before_date: String,
    #[serde(default)]
    easy_days_percentages: Vec<f32>,
    #[serde(default)]
    stop_timer_on_answer: bool,
    #[serde(default)]
    seconds_to_show_question: f32,
    #[serde(default)]
    seconds_to_show_answer: f32,
    #[serde(default)]
    question_action: QuestionAction,
    #[serde(default, deserialize_with = "default_on_invalid")]
    answer_action: AnswerAction,
    #[serde(default = "wait_for_audio_default")]
    wait_for_audio: bool,
    #[serde(default)]
    /// historical retention
    sm2_retention: f32,
    #[serde(default, rename = "weightSearch")]
    param_search: String,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}
#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
#[derive(Default)]
pub enum QuestionAction {
    #[default]
    ShowAnswer = 0,
    ShowReminder = 1,
}

#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
#[derive(Default)]
pub enum AnswerAction {
    #[default]
    ShowReminder = 0,
    AnswerAgain = 1,
    AnswerGood = 2,
    AnswerHard = 3,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewConfSchema11 {
    #[serde(deserialize_with = "default_on_invalid")]
    delays: Vec<f32>,
    initial_factor: u16,
    #[serde(deserialize_with = "deserialize_new_intervals")]
    ints: NewCardIntervals,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) order: NewCardOrderSchema11,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) per_day: u32,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}

#[derive(Serialize_tuple, Debug, PartialEq, Eq, Clone)]
pub struct NewCardIntervals {
    good: u16,
    easy: u16,
    _unused: u16,
}

impl Default for NewCardIntervals {
    fn default() -> Self {
        Self {
            good: 1,
            easy: 4,
            _unused: 0,
        }
    }
}

/// This extra logic is required because AnkiDroid's options screen was creating
/// a 2 element array instead of a 3 element one.
fn deserialize_new_intervals<'de, D>(deserializer: D) -> Result<NewCardIntervals, D::Error>
where
    D: Deserializer<'de>,
{
    let vals: Result<Vec<u16>, _> = DeTrait::deserialize(deserializer);
    Ok(vals
        .ok()
        .and_then(|vals| {
            if vals.len() >= 2 {
                Some(NewCardIntervals {
                    good: vals[0],
                    easy: vals[1],
                    _unused: 0,
                })
            } else {
                None
            }
        })
        .unwrap_or_default())
}

#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
#[derive(Default)]
pub enum NewCardOrderSchema11 {
    Random = 0,
    #[default]
    Due = 1,
}

fn hard_factor_default() -> f32 {
    1.2
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RevConfSchema11 {
    ease4: f32,
    ivl_fct: f32,
    max_ivl: u32,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) per_day: u32,
    #[serde(default = "hard_factor_default")]
    hard_factor: f32,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LapseConfSchema11 {
    #[serde(deserialize_with = "default_on_invalid")]
    delays: Vec<f32>,
    min_int: u32,
    mult: f32,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}

impl Default for RevConfSchema11 {
    fn default() -> Self {
        RevConfSchema11 {
            ease4: 1.3,
            ivl_fct: 1.0,
            max_ivl: 36500,
            per_day: 200,
            hard_factor: 1.2,
            other: Default::default(),
        }
    }
}

impl Default for NewConfSchema11 {
    fn default() -> Self {
        NewConfSchema11 {
            delays: vec![1.0, 10.0],
            initial_factor: INITIAL_EASE_FACTOR_THOUSANDS,
            ints: NewCardIntervals::default(),
            order: NewCardOrderSchema11::default(),
            per_day: 20,
            other: Default::default(),
        }
    }
}

impl Default for LapseConfSchema11 {
    fn default() -> Self {
        LapseConfSchema11 {
            delays: vec![10.0],
            min_int: 1,
            mult: 0.0,
            other: Default::default(),
        }
    }
}

// schema11 -> schema15

impl From<DeckConfSchema11> for DeckConfig {
    fn from(mut c: DeckConfSchema11) -> DeckConfig {
        // merge any json stored in new/rev/lapse into top level
        if !c.new.other.is_empty() {
            if let Ok(val) = serde_json::to_value(c.new.other) {
                c.other.insert("new".into(), val);
            }
        }
        if !c.rev.other.is_empty() {
            if let Ok(val) = serde_json::to_value(c.rev.other) {
                c.other.insert("rev".into(), val);
            }
        }
        if !c.lapse.other.is_empty() {
            if let Ok(val) = serde_json::to_value(c.lapse.other) {
                c.other.insert("lapse".into(), val);
            }
        }
        let other_bytes = other_to_bytes_without_usn(c.other);

        DeckConfig {
            id: c.id,
            name: c.name,
            mtime_secs: c.mtime,
            inner: DeckConfigInner {
                learn_steps: c.new.delays,
                relearn_steps: c.lapse.delays,
                new_per_day: c.new.per_day,
                reviews_per_day: c.rev.per_day,
                new_per_day_minimum: c.new_per_day_minimum,
                maximum_review_interval: c.rev.max_ivl,
                new_card_insert_order: match c.new.order {
                    NewCardOrderSchema11::Random => NewCardInsertOrder::Random,
                    NewCardOrderSchema11::Due => NewCardInsertOrder::Due,
                } as i32,
                new_card_gather_priority: c.new_gather_priority,
                new_card_sort_order: c.new_sort_order,
                review_order: c.review_order,
                new_mix: c.new_mix,
                interday_learning_mix: c.interday_learning_mix,
                disable_autoplay: !c.autoplay,
                cap_answer_time_to_secs: c.max_taken.max(0) as u32,
                show_timer: c.timer != 0,
                stop_timer_on_answer: c.stop_timer_on_answer,
                seconds_to_show_question: c.seconds_to_show_question,
                seconds_to_show_answer: c.seconds_to_show_answer,
                question_action: c.question_action as i32,
                answer_action: c.answer_action as i32,
                wait_for_audio: c.wait_for_audio,
                skip_question_when_replaying_answer: !c.replayq,
                fsrs_params_4: c.fsrs_params_4,
                fsrs_params_5: c.fsrs_params_5,
                fsrs_params_6: c.fsrs_params_6,
                ignore_revlogs_before_date: c.ignore_revlogs_before_date,
                easy_days_percentages: c.easy_days_percentages,
                desired_retention: c.desired_retention,
                historical_retention: c.sm2_retention,
                param_search: c.param_search,
                other: other_bytes,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use serde::de::IntoDeserializer;
    use serde_json::json;
    use serde_json::Value;

    use super::*;

    #[test]
    fn new_intervals() {
        let decode = |value: Value| -> NewCardIntervals {
            deserialize_new_intervals(value.into_deserializer()).unwrap()
        };
        assert_eq!(
            decode(json!([2, 4, 6])),
            NewCardIntervals {
                good: 2,
                easy: 4,
                _unused: 0
            }
        );
        assert_eq!(
            decode(json!([3, 9])),
            NewCardIntervals {
                good: 3,
                easy: 9,
                _unused: 0
            }
        );
        // invalid input will yield defaults
        assert_eq!(
            decode(json!([4])),
            NewCardIntervals {
                good: 1,
                easy: 4,
                _unused: 0
            }
        );
        assert_eq!(
            decode(json!([-5, 4, 3])),
            NewCardIntervals {
                good: 1,
                easy: 4,
                _unused: 0
            }
        );
    }
}
