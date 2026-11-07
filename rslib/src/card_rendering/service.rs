// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_proto::card_rendering::ExtractClozeForTypingRequest;
use anki_proto::generic;

use crate::card::CardId;
use crate::card_rendering::extract_av_tags;
use crate::cloze::extract_cloze_for_typing;
use crate::collection::Collection;
use crate::error::Result;
use crate::markdown::render_markdown;
use crate::notetype::RenderCardOutput;
use crate::template::RenderedNode;
use crate::text::encode_iri_paths;
use crate::text::sanitize_html_no_images;
use crate::typeanswer::compare_answer;

/// While the majority of these methods do not actually require a collection,
/// they are unlikely to be executed without one, so we only bother implementing
/// them for the collection.
impl crate::services::CardRenderingService for Collection {
    fn extract_av_tags(
        &mut self,
        input: anki_proto::card_rendering::ExtractAvTagsRequest,
    ) -> Result<anki_proto::card_rendering::ExtractAvTagsResponse> {
        let out = extract_av_tags(input.text, input.question_side);
        Ok(anki_proto::card_rendering::ExtractAvTagsResponse {
            text: out.0,
            av_tags: out.1,
        })
    }

    fn get_empty_cards(&mut self) -> Result<anki_proto::card_rendering::EmptyCardsReport> {
        let mut empty = self.empty_cards()?;
        let report = self.empty_cards_report(&mut empty)?;

        let mut outnotes = vec![];
        for (_ntid, notes) in empty {
            outnotes.extend(notes.into_iter().map(|e| {
                anki_proto::card_rendering::empty_cards_report::NoteWithEmptyCards {
                    note_id: e.nid.0,
                    will_delete_note: e.empty.len() == e.current_count,
                    card_ids: e.empty.into_iter().map(|(_ord, id)| id.0).collect(),
                }
            }))
        }
        Ok(anki_proto::card_rendering::EmptyCardsReport {
            report,
            notes: outnotes,
        })
    }

    fn render_existing_card(
        &mut self,
        input: anki_proto::card_rendering::RenderExistingCardRequest,
    ) -> Result<anki_proto::card_rendering::RenderCardResponse> {
        self.render_existing_card(CardId(input.card_id), input.browser, input.partial_render)
            .map(Into::into)
    }

    fn render_markdown(
        &mut self,
        input: anki_proto::card_rendering::RenderMarkdownRequest,
    ) -> Result<generic::String> {
        let mut text = render_markdown(&input.markdown);
        if input.sanitize {
            // currently no images
            text = sanitize_html_no_images(&text);
        }
        Ok(text.into())
    }

    fn encode_iri_paths(&mut self, input: generic::String) -> Result<generic::String> {
        Ok(encode_iri_paths(&input.val).to_string().into())
    }

    fn compare_answer(
        &mut self,
        input: anki_proto::card_rendering::CompareAnswerRequest,
    ) -> Result<generic::String> {
        Ok(compare_answer(
            &input.expected,
            &input.provided,
            input.combining,
            input.ignore_case,
        )
        .into())
    }

    fn extract_cloze_for_typing(
        &mut self,
        input: ExtractClozeForTypingRequest,
    ) -> Result<generic::String> {
        Ok(extract_cloze_for_typing(&input.text, input.ordinal as u16)
            .to_string()
            .into())
    }
}

fn rendered_nodes_to_proto(
    nodes: Vec<RenderedNode>,
) -> Vec<anki_proto::card_rendering::RenderedTemplateNode> {
    nodes
        .into_iter()
        .map(|n| anki_proto::card_rendering::RenderedTemplateNode {
            value: Some(rendered_node_to_proto(n)),
        })
        .collect()
}

fn rendered_node_to_proto(
    node: RenderedNode,
) -> anki_proto::card_rendering::rendered_template_node::Value {
    match node {
        RenderedNode::Text { text } => {
            anki_proto::card_rendering::rendered_template_node::Value::Text(text)
        }
        RenderedNode::Replacement {
            field_name,
            current_text,
            filters,
        } => anki_proto::card_rendering::rendered_template_node::Value::Replacement(
            anki_proto::card_rendering::RenderedTemplateReplacement {
                field_name,
                current_text,
                filters,
            },
        ),
    }
}

impl From<RenderCardOutput> for anki_proto::card_rendering::RenderCardResponse {
    fn from(o: RenderCardOutput) -> Self {
        anki_proto::card_rendering::RenderCardResponse {
            question_nodes: rendered_nodes_to_proto(o.qnodes),
            answer_nodes: rendered_nodes_to_proto(o.anodes),
            css: o.css,
            is_empty: o.is_empty,
        }
    }
}

#[cfg(test)]
mod test {
    use anki_proto::card_rendering::rendered_template_node::Value;
    use anki_proto::card_rendering::CompareAnswerRequest;
    use anki_proto::card_rendering::ExtractAvTagsRequest;
    use anki_proto::card_rendering::ExtractClozeForTypingRequest;
    use anki_proto::card_rendering::RenderExistingCardRequest;
    use anki_proto::card_rendering::RenderMarkdownRequest;

    use crate::error::AnkiError;
    use crate::prelude::*;
    use crate::services::CardRenderingService;
    use crate::tests::NoteAdder;

    /// The text of a node list that rendered to a single text node, or `None`
    /// otherwise (an empty list, multiple nodes, or a replacement node — i.e.
    /// partially rendered output).
    fn text_of(nodes: &[anki_proto::card_rendering::RenderedTemplateNode]) -> Option<&str> {
        match nodes {
            [node] => match node.value.as_ref() {
                Some(Value::Text(text)) => Some(text),
                _ => None,
            },
            _ => None,
        }
    }

    // Question/answer rendering.

    #[test]
    fn render_existing_card_returns_saved_card_content() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col)
            .fields(&["front", "back"])
            .add(&mut col);
        let card_id = col.get_first_card().id.0;

        let resp = CardRenderingService::render_existing_card(
            &mut col,
            RenderExistingCardRequest {
                card_id,
                browser: false,
                partial_render: false,
            },
        )
        .unwrap();

        assert_eq!(text_of(&resp.question_nodes), Some("front"));
        assert_eq!(
            text_of(&resp.answer_nodes),
            Some("front\n\n<hr id=answer>\n\nback")
        );
    }

    // Template filters.

    // Edge cases (empty fields and cloze).

    // Error handling (structured errors, no panic).

    #[test]
    fn card_flag_field_renders_empty_and_new_cards_have_no_flags() {
        let mut col = Collection::new();
        let mut notetype = (*col.get_notetype_by_name("Basic").unwrap().unwrap()).clone();
        notetype.templates[0].config.q_format = "{{Front}}[{{CardFlag}}]".into();
        col.update_notetype(&mut notetype, false).unwrap();
        NoteAdder::basic(&mut col)
            .fields(&["front", "back"])
            .add(&mut col);
        let card_id = col.get_first_card().id.0;

        let resp = CardRenderingService::render_existing_card(
            &mut col,
            RenderExistingCardRequest {
                card_id,
                browser: false,
                partial_render: false,
            },
        )
        .unwrap();

        assert_eq!(text_of(&resp.question_nodes), Some("front[]"));
        let flags: u32 = col.storage.db_scalar("select flags from cards").unwrap();
        assert_eq!(flags, 0);
    }

    #[test]
    fn render_existing_card_errors_for_unknown_card_id() {
        let mut col = Collection::new();

        let err = CardRenderingService::render_existing_card(
            &mut col,
            RenderExistingCardRequest {
                card_id: 12345,
                browser: false,
                partial_render: false,
            },
        )
        .unwrap_err();

        match err {
            AnkiError::InvalidInput { source } => {
                assert_eq!(source.message(), "no such card");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    // Remaining delegating methods: assert the bridge contract
    // (proto conversion, mode/flag dispatch) rather than re-testing the
    // domain rules, which are covered by the free functions' own unit tests.

    #[test]
    fn extract_av_tags_moves_sound_into_av_tags() {
        let mut col = Collection::new();

        let resp = col
            .extract_av_tags(ExtractAvTagsRequest {
                text: "foo [sound:bar.mp3] baz".into(),
                question_side: true,
            })
            .unwrap();

        assert_eq!(resp.text, "foo [anki:play:q:0] baz");
        assert_eq!(resp.av_tags.len(), 1);
        assert_eq!(resp.av_tags[0].sound, "bar.mp3");
    }

    #[test]
    fn get_empty_cards_reports_note_with_empty_card() {
        let mut col = Collection::new();
        let nt = col.basic_rev_notetype();
        // Generate both cards with the fields filled, then blank the Back field:
        // the reverse card now renders empty while the forward card keeps the
        // note alive.
        let mut note = NoteAdder::new(&nt).fields(&["front", "back"]).add(&mut col);
        let reverse_card_id = col
            .storage
            .all_cards_of_note(note.id)
            .unwrap()
            .into_iter()
            .find(|card| card.template_idx == 1)
            .expect("expected a reverse card")
            .id
            .0;
        note.set_field(1, "").unwrap();
        col.update_note(&mut note).unwrap();

        let report = col.get_empty_cards().unwrap();

        assert_eq!(report.notes.len(), 1);
        assert_eq!(report.notes[0].note_id, note.id.0);
        assert_eq!(report.notes[0].card_ids, vec![reverse_card_id]);
        assert!(!report.notes[0].will_delete_note);
        assert!(!report.report.is_empty());
    }

    #[test]
    fn get_empty_cards_marks_note_for_deletion_when_all_cards_empty() {
        let mut col = Collection::new();
        let mut note = NoteAdder::basic(&mut col)
            .fields(&["front", "back"])
            .add(&mut col);
        let card_id = col.storage.all_cards_of_note(note.id).unwrap()[0].id.0;
        note.set_field(0, "").unwrap();
        note.set_field(1, "").unwrap();
        col.update_note(&mut note).unwrap();

        let report = col.get_empty_cards().unwrap();

        assert_eq!(report.notes.len(), 1);
        assert_eq!(report.notes[0].note_id, note.id.0);
        assert_eq!(report.notes[0].card_ids, vec![card_id]);
        assert!(report.notes[0].will_delete_note);
    }

    #[test]
    fn render_markdown_converts_without_sanitizing() {
        let mut col = Collection::new();

        let resp = col
            .render_markdown(RenderMarkdownRequest {
                markdown: "# Title".into(),
                sanitize: false,
            })
            .unwrap();

        assert_eq!(resp.val, "<h1>Title</h1>\n");
    }

    #[test]
    fn render_markdown_sanitizes_disallowed_markup() {
        let mut col = Collection::new();

        let resp = col
            .render_markdown(RenderMarkdownRequest {
                markdown: "safe\n\n<script>alert(1)</script>".into(),
                sanitize: true,
            })
            .unwrap();

        assert!(resp.val.contains("safe"));
        assert!(!resp.val.contains("<script>"));
        assert!(!resp.val.contains("alert(1)"));
    }

    #[test]
    fn compare_answer_marks_correct_input() {
        let mut col = Collection::new();

        let resp = col
            .compare_answer(CompareAnswerRequest {
                expected: "foo".into(),
                provided: "foo".into(),
                combining: true,
                ignore_case: false,
            })
            .unwrap();

        assert_eq!(
            resp.val,
            "<code id=typeans><span class=typeGood>foo</span></code>"
        );
    }

    #[test]
    fn extract_cloze_for_typing_returns_answer_for_ordinal() {
        let mut col = Collection::new();

        let resp = col
            .extract_cloze_for_typing(ExtractClozeForTypingRequest {
                text: "{{c1::foo}} {{c2::bar}}".into(),
                ordinal: 1,
            })
            .unwrap();

        assert_eq!(resp.val, "foo");
    }
}
