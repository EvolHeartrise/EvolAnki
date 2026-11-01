// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::borrow::Cow;
use std::collections::HashMap;

use super::CardTemplate;
use super::Notetype;
use super::NotetypeKind;
use crate::prelude::*;
use crate::template::render_card;
use crate::template::RenderCardRequest;
use crate::template::RenderedNode;

#[derive(Debug)]
pub struct RenderCardOutput {
    pub qnodes: Vec<RenderedNode>,
    pub anodes: Vec<RenderedNode>,
    pub css: String,
    pub latex_svg: bool,
    pub is_empty: bool,
}

impl RenderCardOutput {

}

impl Collection {
    /// Render an existing card saved in the database.
    pub fn render_existing_card(
        &mut self,
        cid: CardId,
        browser: bool,
        partial_render: bool,
    ) -> Result<RenderCardOutput> {
        let card = self.storage.get_card(cid)?.or_invalid("no such card")?;
        let note = self
            .storage
            .get_note(card.note_id)?
            .or_invalid("no such note")?;
        let nt = self
            .get_notetype(note.notetype_id)?
            .or_invalid("no such notetype")?;
        let template = match nt.config.kind() {
            NotetypeKind::Normal => nt.templates.get(card.template_idx as usize),
            NotetypeKind::Cloze => nt.templates.first(),
        }
        .or_invalid("missing template")?;

        self.render_card(&note, &card, &nt, template, browser, partial_render)
    }

    pub fn render_card(
        &mut self,
        note: &Note,
        card: &Card,
        nt: &Notetype,
        template: &CardTemplate,
        browser: bool,
        partial_render: bool,
    ) -> Result<RenderCardOutput> {
        let mut field_map = note.fields_map(&nt.fields);

        self.add_special_fields(&mut field_map, note, card, nt, template)?;
        // due to lifetime restrictions we need to add card number here
        let card_num = format!("c{}", card.template_idx + 1);
        field_map.entry(&card_num).or_insert_with(|| "1".into());

        let (qfmt, afmt) = if browser {
            (
                template.question_format_for_browser(),
                template.answer_format_for_browser(),
            )
        } else {
            (
                template.config.q_format.as_str(),
                template.config.a_format.as_str(),
            )
        };

        let response = render_card(RenderCardRequest {
            qfmt,
            afmt,
            field_map: &field_map,
            card_ord: card.template_idx,
            is_cloze: nt.is_cloze(),
            browser,
            tr: &self.tr,
            partial_render,
        })?;
        Ok(RenderCardOutput {
            qnodes: response.qnodes,
            anodes: response.anodes,
            css: nt.config.css.clone(),
            latex_svg: nt.config.latex_svg,
            is_empty: response.is_empty,
        })
    }

    /// Add special fields if they don't clobber note fields.
    /// The fields supported here must coincide with SPECIAL_FIELDS in
    /// notetype/mod.rs, apart from FrontSide which is handled by Python.
    fn add_special_fields(
        &mut self,
        map: &mut HashMap<&str, Cow<str>>,
        note: &Note,
        card: &Card,
        nt: &Notetype,
        template: &CardTemplate,
    ) -> Result<()> {
        let tags = note.tags.join(" ");
        map.entry("Tags").or_insert_with(|| tags.into());
        map.entry("Type").or_insert_with(|| nt.name.clone().into());
        let deck_name: Cow<str> = self
            .get_deck(card.original_deck_id.or(card.deck_id))?
            .map(|d| d.human_name().into())
            .unwrap_or_else(|| "(Deck)".into());
        let subdeck_name = deck_name.rsplit("::").next().unwrap();
        map.entry("Subdeck")
            .or_insert_with(|| subdeck_name.to_string().into());
        map.entry("Deck")
            .or_insert_with(|| deck_name.to_string().into());
        // Flags are gone, but a template from elsewhere may name this. Without it,
        // the card wouldn't render at all.
        map.entry("CardFlag").or_insert_with(|| "".into());
        map.entry("Card")
            .or_insert_with(|| template.name.clone().into());
        map.entry("CardID")
            .or_insert_with(|| card.id.to_string().into());

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::collection::CollectionBuilder;
    use crate::notetype::SPECIAL_FIELDS;

    #[test]
    fn special_fields_complete() -> Result<()> {
        let mut col = CollectionBuilder::default().build()?;
        let mut map = HashMap::new();

        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let note = Note::new(&nt);
        let card = Card::new(0.into(), 0.try_into().unwrap(), 0.into(), 0);
        let tmpl = nt.templates[0].clone();

        col.add_special_fields(&mut map, &note, &card, &nt, &tmpl)?;

        assert!(map.iter().all(|val| SPECIAL_FIELDS.contains(val.0)));
        Ok(())
    }
}
