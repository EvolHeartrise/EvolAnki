// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use anki_proto::generic;

use crate::collection::Collection;
use crate::error;
use crate::error::OrNotFound;
use crate::notetype::Notetype;
use crate::notetype::NotetypeId;

impl crate::services::NotetypesService for Collection {
    fn add_notetype(
        &mut self,
        input: anki_proto::notetypes::Notetype,
    ) -> error::Result<anki_proto::collection::OpChangesWithId> {
        let mut notetype: Notetype = input.into();

        Ok(self
            .add_notetype(&mut notetype, false)?
            .map(|_| notetype.id.0)
            .into())
    }

    fn update_notetype(
        &mut self,
        input: anki_proto::notetypes::Notetype,
    ) -> error::Result<anki_proto::collection::OpChanges> {
        let mut notetype: Notetype = input.into();
        self.update_notetype(&mut notetype, false).map(Into::into)
    }

    fn get_notetype(
        &mut self,
        input: anki_proto::notetypes::NotetypeId,
    ) -> error::Result<anki_proto::notetypes::Notetype> {
        let ntid = input.into();

        self.storage
            .get_notetype(ntid)?
            .or_not_found(ntid)
            .map(Into::into)
    }

    fn get_notetype_names(&mut self) -> error::Result<anki_proto::notetypes::NotetypeNames> {
        let entries: Vec<_> = self
            .storage
            .get_all_notetype_names()?
            .into_iter()
            .map(|(id, name)| anki_proto::notetypes::NotetypeNameId { id: id.0, name })
            .collect();
        Ok(anki_proto::notetypes::NotetypeNames { entries })
    }

    fn get_notetype_names_and_counts(
        &mut self,
    ) -> error::Result<anki_proto::notetypes::NotetypeUseCounts> {
        let entries: Vec<_> = self
            .storage
            .get_notetype_use_counts()?
            .into_iter()
            .map(
                |(id, name, use_count)| anki_proto::notetypes::NotetypeNameIdUseCount {
                    id: id.0,
                    name,
                    use_count,
                },
            )
            .collect();
        Ok(anki_proto::notetypes::NotetypeUseCounts { entries })
    }

    fn get_notetype_id_by_name(
        &mut self,
        input: generic::String,
    ) -> error::Result<anki_proto::notetypes::NotetypeId> {
        self.storage
            .get_notetype_id(&input.val)
            .and_then(|nt| nt.or_not_found(input.val))
            .map(|ntid| anki_proto::notetypes::NotetypeId { ntid: ntid.0 })
    }
}

impl From<anki_proto::notetypes::Notetype> for Notetype {
    fn from(n: anki_proto::notetypes::Notetype) -> Self {
        Notetype {
            id: n.id.into(),
            name: n.name,
            mtime_secs: n.mtime_secs.into(),
            usn: n.usn.into(),
            fields: n.fields.into_iter().map(Into::into).collect(),
            templates: n.templates.into_iter().map(Into::into).collect(),
            config: n.config.unwrap_or_default(),
        }
    }
}

impl From<anki_proto::notetypes::NotetypeId> for NotetypeId {
    fn from(ntid: anki_proto::notetypes::NotetypeId) -> Self {
        NotetypeId(ntid.ntid)
    }
}

impl From<NotetypeId> for anki_proto::notetypes::NotetypeId {
    fn from(ntid: NotetypeId) -> Self {
        anki_proto::notetypes::NotetypeId { ntid: ntid.0 }
    }
}

#[cfg(test)]
mod tests {
    use anki_proto::generic;

    use super::*;
    use crate::services::NotetypesService;
    use crate::tests::NoteAdder;

    fn basic_notetype_id(col: &mut Collection) -> i64 {
        NotetypesService::get_notetype_names(col)
            .unwrap()
            .entries
            .into_iter()
            .find(|e| e.name == "Basic")
            .unwrap()
            .id
    }

    // --- From<proto::NotetypeId> for NotetypeId ---

    #[test]
    fn proto_notetype_id_converts_to_domain() {
        let proto = anki_proto::notetypes::NotetypeId { ntid: 42 };
        let domain: NotetypeId = proto.into();
        assert_eq!(domain, NotetypeId(42));
    }

    #[test]
    fn notetype_id_round_trips_through_proto() {
        let original = NotetypeId(99);
        let proto: anki_proto::notetypes::NotetypeId = original.into();
        let back: NotetypeId = proto.into();
        assert_eq!(back, original);
    }

    // --- From<ChangeNotetypeRequest> for ChangeNotetypeInput ---

    // --- From<ChangeNotetypeInput> for ChangeNotetypeRequest ---

    // --- Service: get_notetype_legacy / get_stock_notetype_legacy ---

    // --- Service: add_notetype_legacy ---

    // --- Service: update_notetype_legacy ---

    // --- Service: add_or_update_notetype (branch: id=0 → add) ---

    // --- Service: add_or_update_notetype (branch: id!=0, !preserve_usn_and_mtime →
    // update) ---

    // --- Service: add_or_update_notetype (branch: id!=0, preserve_usn_and_mtime →
    // add_or_update_with_existing_id) ---

    // --- Service: get_notetype_names ---

    #[test]
    fn get_notetype_names_includes_stock_notetypes() {
        let mut col = Collection::new();
        let names = NotetypesService::get_notetype_names(&mut col).unwrap();
        let name_list: Vec<&str> = names.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(name_list.contains(&"Basic"), "expected Basic in names");
        assert!(name_list.contains(&"Cloze"), "expected Cloze in names");
    }

    // --- Service: add_notetype ---

    #[test]
    fn add_notetype_returns_non_zero_id() {
        let mut col = Collection::new();
        let basic_id = basic_notetype_id(&mut col);
        let mut proto = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        proto.id = 0;
        proto.name = "TestType".to_string();
        let result = NotetypesService::add_notetype(&mut col, proto).unwrap();
        assert!(result.id > 0, "expected non-zero ID after add");
    }

    #[test]
    fn add_notetype_is_retrievable_by_id() {
        let mut col = Collection::new();
        let basic_id = basic_notetype_id(&mut col);
        let mut proto = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        proto.id = 0;
        proto.name = "RetrieveById".to_string();
        let result = NotetypesService::add_notetype(&mut col, proto).unwrap();
        let fetched = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: result.id },
        )
        .unwrap();
        assert_eq!(fetched.name, "RetrieveById");
    }

    #[test]
    fn add_notetype_is_retrievable_by_name() {
        let mut col = Collection::new();
        let basic_id = basic_notetype_id(&mut col);
        let mut proto = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        proto.id = 0;
        proto.name = "RetrieveByName".to_string();
        let _ = NotetypesService::add_notetype(&mut col, proto).unwrap();
        let result = NotetypesService::get_notetype_id_by_name(
            &mut col,
            generic::String {
                val: "RetrieveByName".to_string(),
            },
        )
        .unwrap();
        assert!(
            result.ntid > 0,
            "expected non-zero ID from get_notetype_id_by_name"
        );
    }

    // --- Service: get_notetype error path ---

    #[test]
    fn get_notetype_returns_error_for_unknown_id() {
        let mut col = Collection::new();
        let result = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: 999_999 },
        );
        assert!(result.is_err(), "expected Err for unknown notetype ID");
    }

    // --- Service: get_notetype_id_by_name error path ---

    #[test]
    fn get_notetype_id_by_name_returns_error_for_unknown_name() {
        let mut col = Collection::new();
        let result = NotetypesService::get_notetype_id_by_name(
            &mut col,
            generic::String {
                val: "NonExistentNotetype".to_string(),
            },
        );
        assert!(result.is_err(), "expected Err for unknown notetype name");
    }

    // --- Service: get_field_names ---

    // --- Service: update_notetype ---

    #[test]
    fn update_notetype_field_name_persists() {
        let mut col = Collection::new();
        let basic_id = basic_notetype_id(&mut col);
        let mut proto = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        proto.fields[0].name = "Question".to_string();
        let _ = NotetypesService::update_notetype(&mut col, proto).unwrap();
        let updated = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        assert_eq!(
            updated.fields[0].name, "Question",
            "expected renamed field to persist after update"
        );
    }

    #[test]
    fn update_notetype_adding_template_generates_new_card() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col)
            .fields(&["front text", "back text"])
            .add(&mut col);
        assert_eq!(
            col.storage.get_all_cards().len(),
            1,
            "expected 1 card before adding template"
        );
        let basic_id = basic_notetype_id(&mut col);
        let mut proto = NotetypesService::get_notetype(
            &mut col,
            anki_proto::notetypes::NotetypeId { ntid: basic_id },
        )
        .unwrap();
        proto
            .templates
            .push(anki_proto::notetypes::notetype::Template {
                name: "Card 2".to_string(),
                config: Some(anki_proto::notetypes::notetype::template::Config {
                    q_format: "{{Back}}".to_string(),
                    a_format: "{{FrontSide}}<hr id=answer>{{Front}}".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        let _ = NotetypesService::update_notetype(&mut col, proto).unwrap();
        assert_eq!(
            col.storage.get_all_cards().len(),
            2,
            "expected 2 cards after adding template"
        );
    }

    // --- Service: remove_notetype ---

    // --- Service: get_notetype_names_and_counts ---

    #[test]
    fn get_notetype_names_and_counts_returns_zero_for_unused_notetypes() {
        let mut col = Collection::new();
        let counts = NotetypesService::get_notetype_names_and_counts(&mut col).unwrap();
        let basic = counts
            .entries
            .iter()
            .find(|e| e.name == "Basic")
            .expect("Basic notetype not found");
        assert_eq!(
            basic.use_count, 0,
            "expected use_count 0 for Basic with no notes"
        );
    }

    #[test]
    fn get_notetype_names_and_counts_increments_after_adding_note() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);
        let counts = NotetypesService::get_notetype_names_and_counts(&mut col).unwrap();
        let basic = counts
            .entries
            .iter()
            .find(|e| e.name == "Basic")
            .expect("Basic notetype not found");
        assert_eq!(
            basic.use_count, 1,
            "expected use_count 1 after adding one Basic note"
        );
    }

    // --- Service: get_aux_notetype_config_key ---

    // --- Service: get_aux_template_config_key ---
}
