// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use anki_proto::generic;
use anki_proto::import_export::import_response::Log as NoteLog;
use anki_proto::import_export::ExportLimit;

use crate::prelude::*;
use crate::search::SearchNode;

impl crate::services::ImportExportService for Collection {
    fn import_anki_package(
        &mut self,
        input: anki_proto::import_export::ImportAnkiPackageRequest,
    ) -> Result<anki_proto::import_export::ImportResponse> {
        self.import_apkg(&input.package_path, input.options.unwrap_or_default())
            .map(Into::into)
    }

    fn export_anki_package(
        &mut self,
        input: anki_proto::import_export::ExportAnkiPackageRequest,
    ) -> Result<generic::UInt32> {
        self.export_apkg(
            &input.out_path,
            input.options.unwrap_or_default(),
            input.limit.unwrap_or_default(),
            None,
        )
        .map(Into::into)
    }

    fn get_csv_metadata(
        &mut self,
        input: anki_proto::import_export::CsvMetadataRequest,
    ) -> Result<anki_proto::import_export::CsvMetadata> {
        let delimiter = input.delimiter.is_some().then(|| input.delimiter());

        self.get_csv_metadata(
            &input.path,
            delimiter,
            input.notetype_id.map(Into::into),
            input.deck_id.map(Into::into),
            input.is_html,
        )
    }

    fn import_csv(
        &mut self,
        input: anki_proto::import_export::ImportCsvRequest,
    ) -> Result<anki_proto::import_export::ImportResponse> {
        self.import_csv(&input.path, input.metadata.unwrap_or_default())
            .map(Into::into)
    }
}

impl From<OpOutput<NoteLog>> for anki_proto::import_export::ImportResponse {
    fn from(output: OpOutput<NoteLog>) -> Self {
        Self {
            changes: Some(output.changes.into()),
            log: Some(output.output),
        }
    }
}

impl From<ExportLimit> for SearchNode {
    fn from(export_limit: ExportLimit) -> Self {
        use anki_proto::import_export::export_limit::Limit;
        let limit = export_limit
            .limit
            .unwrap_or(Limit::WholeCollection(generic::Empty {}));
        match limit {
            Limit::WholeCollection(_) => Self::WholeCollection,
            Limit::DeckId(did) => Self::from_deck_id(did, true),
            Limit::NoteIds(nids) => Self::from_note_ids(nids.note_ids),
            Limit::CardIds(cids) => Self::from_card_ids(cids.cids),
        }
    }
}

#[cfg(test)]
mod tests {

    use anki_proto::import_export::export_limit::Limit;
    use anki_proto::import_export::import_response::Log as NoteLog;
    use anki_proto::import_export::CsvMetadataRequest;
    use anki_proto::import_export::ExportAnkiPackageRequest;
    use anki_proto::import_export::ExportLimit;
    use anki_proto::import_export::ImportAnkiPackageRequest;
    use anki_proto::import_export::ImportCsvRequest;
    use tempfile::tempdir;

    use super::*;
    use crate::ops::Op;
    use crate::ops::OpChanges;
    use crate::ops::OpOutput;
    use crate::ops::StateChanges;
    use crate::services::ImportExportService;

    // --- From<ExportLimit> for SearchNode ---

    #[test]
    fn export_limit_none_becomes_whole_collection() {
        let node = SearchNode::from(ExportLimit { limit: None });
        assert_eq!(node, SearchNode::WholeCollection);
    }

    #[test]
    fn export_limit_note_ids_becomes_note_id_search() {
        use anki_proto::notes::NoteIds;
        let node = SearchNode::from(ExportLimit {
            limit: Some(Limit::NoteIds(NoteIds {
                note_ids: vec![1, 2, 3],
            })),
        });
        assert!(
            matches!(node, SearchNode::NoteIds(_)),
            "expected NoteIds search node, got: {node:?}"
        );
    }

    #[test]
    fn export_limit_card_ids_becomes_card_id_search() {
        use anki_proto::cards::CardIds;
        let node = SearchNode::from(ExportLimit {
            limit: Some(Limit::CardIds(CardIds { cids: vec![10, 20] })),
        });
        assert!(
            matches!(node, SearchNode::CardIds(_)),
            "expected CardIds search node, got: {node:?}"
        );
    }

    #[test]
    fn export_limit_deck_id_becomes_deck_id_with_children() {
        let node = SearchNode::from(ExportLimit {
            limit: Some(Limit::DeckId(42)),
        });
        assert!(
            matches!(node, SearchNode::DeckIdWithChildren(_)),
            "expected DeckIdWithChildren, got: {node:?}"
        );
    }

    // --- Service function tests ---

    #[test]
    fn import_anki_package_succeeds_with_valid_apkg_fixture() {
        let apkg_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/update1.apkg");
        let (mut col, _dir) = crate::tests::open_fs_test_collection("import_apkg_success");
        let result = ImportExportService::import_anki_package(
            &mut col,
            ImportAnkiPackageRequest {
                package_path: apkg_path.to_str().unwrap().into(),
                options: None,
            },
        );
        assert!(result.is_ok(), "expected Ok for valid .apkg fixture");
        let response = result.unwrap();
        assert!(response.log.is_some(), "expected import log in response");
    }

    #[test]
    fn import_anki_package_propagates_error_for_missing_file() {
        let mut col = Collection::new();
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.apkg");
        let result = ImportExportService::import_anki_package(
            &mut col,
            ImportAnkiPackageRequest {
                package_path: missing.to_str().unwrap().into(),
                options: None,
            },
        );
        assert!(result.is_err(), "expected Err for missing .apkg file");
    }

    #[test]
    fn export_anki_package_succeeds_with_empty_collection() {
        let mut col = Collection::new();
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.apkg");
        let result = ImportExportService::export_anki_package(
            &mut col,
            ExportAnkiPackageRequest {
                out_path: path.to_str().unwrap().into(),
                options: None,
                limit: None,
            },
        );
        assert!(result.is_ok(), "expected Ok for empty collection export");
        assert!(path.exists(), "expected .apkg file to be created");
    }

    #[test]
    fn export_anki_package_propagates_error_for_invalid_path() {
        let mut col = Collection::new();
        let dir = tempdir().unwrap();
        let bad_path = dir.path().join("no_such_subdir").join("out.apkg");
        let result = ImportExportService::export_anki_package(
            &mut col,
            ExportAnkiPackageRequest {
                out_path: bad_path.to_str().unwrap().into(),
                options: None,
                limit: None,
            },
        );
        assert!(result.is_err(), "expected Err for invalid output path");
    }

    #[test]
    fn import_csv_succeeds_with_valid_csv_file() {
        let mut col = Collection::new();
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.csv");
        // tab-separated: two fields matching Basic notetype (Front, Back)
        // dir and the CSV inside are deleted automatically when dir drops at end of
        // test
        std::fs::write(&path, "front content\tback content\n").unwrap();

        let metadata = ImportExportService::get_csv_metadata(
            &mut col,
            CsvMetadataRequest {
                path: path.to_str().unwrap().into(),
                delimiter: None,
                notetype_id: None,
                deck_id: None,
                is_html: None,
            },
        )
        .unwrap();

        let result = ImportExportService::import_csv(
            &mut col,
            ImportCsvRequest {
                path: path.to_str().unwrap().into(),
                metadata: Some(metadata),
            },
        );
        assert!(result.is_ok(), "expected Ok for valid CSV import");
        assert!(
            result.unwrap().log.is_some(),
            "expected import log in response"
        );
    }

    #[test]
    fn import_csv_propagates_error_for_missing_file() {
        let mut col = Collection::new();
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.csv");
        let result = ImportExportService::import_csv(
            &mut col,
            ImportCsvRequest {
                path: missing.to_str().unwrap().into(),
                metadata: None,
            },
        );
        assert!(result.is_err(), "expected Err for missing CSV file");
    }

    #[test]
    fn op_output_converts_to_import_response_with_changes_and_log() {
        let output = OpOutput {
            output: NoteLog::default(),
            changes: OpChanges {
                op: Op::Import,
                changes: StateChanges::default(),
            },
        };
        let response: anki_proto::import_export::ImportResponse = output.into();
        assert!(response.changes.is_some(), "expected changes to be set");
        assert!(response.log.is_some(), "expected log to be set");
    }
}
