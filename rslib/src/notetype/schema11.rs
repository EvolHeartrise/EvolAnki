// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;
use serde_tuple::Serialize_tuple;

use super::CardRequirementKind;
use super::NotetypeId;
use crate::decks::DeckId;
use crate::notetype::CardRequirement;
use crate::notetype::CardTemplate;
use crate::notetype::CardTemplateConfig;
use crate::notetype::NoteField;
use crate::notetype::NoteFieldConfig;
use crate::notetype::Notetype;
use crate::notetype::NotetypeConfig;
use crate::serde::default_on_invalid;
use crate::serde::deserialize_bool_from_anything;
use crate::serde::deserialize_number_from_string;
use crate::serde::is_default;
use crate::timestamp::TimestampSecs;

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Eq, Debug, Clone)]
#[repr(u8)]
pub enum NotetypeKind {
    Standard = 0,
    Cloze = 1,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotetypeSchema11 {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub(crate) id: NotetypeId,
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) kind: NotetypeKind,
    #[serde(rename = "mod")]
    pub(crate) mtime: TimestampSecs,
    pub(crate) sortf: u16,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) did: Option<DeckId>,
    pub(crate) tmpls: Vec<CardTemplateSchema11>,
    pub(crate) flds: Vec<NoteFieldSchema11>,
    #[serde(deserialize_with = "default_on_invalid")]
    pub(crate) css: String,
    #[serde(default)]
    pub(crate) latex_pre: String,
    #[serde(default)]
    pub(crate) latex_post: String,
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub latexsvg: bool,
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) req: CardRequirementsSchema11,
    #[serde(default, skip_serializing_if = "is_default")]
    pub(crate) original_stock_kind: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub(crate) original_id: Option<i64>,
    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub(crate) struct CardRequirementsSchema11(pub(crate) Vec<CardRequirementSchema11>);

#[derive(Serialize_tuple, Deserialize, Debug, Clone)]
pub(crate) struct CardRequirementSchema11 {
    pub(crate) card_ord: u16,
    pub(crate) kind: FieldRequirementKindSchema11,
    pub(crate) field_ords: Vec<u16>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FieldRequirementKindSchema11 {
    Any,
    All,
    None,
}

impl From<NotetypeSchema11> for Notetype {
    fn from(nt: NotetypeSchema11) -> Self {
        Notetype {
            id: nt.id,
            name: nt.name,
            mtime_secs: nt.mtime,
            config: NotetypeConfig {
                kind: nt.kind as i32,
                sort_field_idx: nt.sortf as u32,
                css: nt.css,
                target_deck_id_unused: nt.did.unwrap_or(DeckId(0)).0,
                latex_pre: nt.latex_pre,
                latex_post: nt.latex_post,
                latex_svg: nt.latexsvg,
                reqs: nt.req.0.into_iter().map(Into::into).collect(),
                original_stock_kind: nt.original_stock_kind,
                original_id: nt.original_id,
                other: other_to_bytes_without_usn(nt.other),
            },
            fields: nt.flds.into_iter().map(Into::into).collect(),
            templates: nt.tmpls.into_iter().map(Into::into).collect(),
        }
    }
}

/// Old JSON has a usn, which isn't kept. As there's no field for it, it would
/// end up with the keys this doesn't know.
pub(crate) fn other_to_bytes_without_usn(mut other: HashMap<String, Value>) -> Vec<u8> {
    other.remove("usn");
    other_to_bytes(&other)
}

fn other_to_bytes(other: &HashMap<String, Value>) -> Vec<u8> {
    if other.is_empty() {
        vec![]
    } else {
        serde_json::to_vec(other).unwrap_or_else(|e| {
            // theoretically should never happen
            println!("serialization failed for {other:?}: {e}");
            vec![]
        })
    }
}

impl From<CardRequirementSchema11> for CardRequirement {
    fn from(r: CardRequirementSchema11) -> Self {
        CardRequirement {
            card_ord: r.card_ord as u32,
            kind: match r.kind {
                FieldRequirementKindSchema11::Any => CardRequirementKind::Any,
                FieldRequirementKindSchema11::All => CardRequirementKind::All,
                FieldRequirementKindSchema11::None => CardRequirementKind::None,
            } as i32,
            field_ords: r.field_ords.into_iter().map(|n| n as u32).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NoteFieldSchema11 {
    pub(crate) name: String,
    pub(crate) ord: Option<u16>,
    #[serde(deserialize_with = "deserialize_bool_from_anything")]
    pub(crate) sticky: bool,
    #[serde(deserialize_with = "deserialize_bool_from_anything")]
    pub(crate) rtl: bool,
    pub(crate) font: String,
    pub(crate) size: u16,

    // These were not in schema 11, but need to be listed here so that the setting is not lost
    // on downgrade/upgrade.
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) description: String,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) plain_text: bool,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) collapsed: bool,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) exclude_from_search: bool,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) id: Option<i64>,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) tag: Option<u32>,

    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) prevent_deletion: bool,

    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
}

impl Default for NoteFieldSchema11 {
    fn default() -> Self {
        Self {
            name: String::new(),
            ord: None,
            sticky: false,
            rtl: false,
            plain_text: false,
            font: "Arial".to_string(),
            size: 20,
            description: String::new(),
            collapsed: false,
            exclude_from_search: false,
            id: None,
            tag: None,
            prevent_deletion: false,
            other: Default::default(),
        }
    }
}

impl From<NoteFieldSchema11> for NoteField {
    fn from(f: NoteFieldSchema11) -> Self {
        NoteField {
            ord: f.ord.map(|o| o as u32),
            name: f.name,
            config: NoteFieldConfig {
                sticky: f.sticky,
                rtl: f.rtl,
                plain_text: f.plain_text,
                font_name: f.font,
                font_size: f.size as u32,
                description: f.description,
                collapsed: f.collapsed,
                exclude_from_search: f.exclude_from_search,
                id: f.id,
                tag: f.tag,
                prevent_deletion: f.prevent_deletion,
                other: other_to_bytes(&f.other),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct CardTemplateSchema11 {
    pub(crate) name: String,
    pub(crate) ord: Option<u16>,
    pub(crate) qfmt: String,
    #[serde(default)]
    pub(crate) afmt: String,
    #[serde(default)]
    pub(crate) bqfmt: String,
    #[serde(default)]
    pub(crate) bafmt: String,
    #[serde(deserialize_with = "default_on_invalid", default)]
    pub(crate) did: Option<DeckId>,
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) bfont: String,
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) bsize: u8,
    #[serde(default, deserialize_with = "default_on_invalid")]
    pub(crate) id: Option<i64>,
    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
}

impl From<CardTemplateSchema11> for CardTemplate {
    fn from(t: CardTemplateSchema11) -> Self {
        CardTemplate {
            ord: t.ord.map(|t| t as u32),
            name: t.name,
            mtime_secs: TimestampSecs(0),
            config: CardTemplateConfig {
                q_format: t.qfmt,
                a_format: t.afmt,
                q_format_browser: t.bqfmt,
                a_format_browser: t.bafmt,
                target_deck_id: t.did.unwrap_or(DeckId(0)).0,
                browser_font_name: t.bfont,
                browser_font_size: t.bsize as u32,
                id: t.id,
                other: other_to_bytes(&t.other),
            },
        }
    }
}
