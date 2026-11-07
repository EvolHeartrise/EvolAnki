// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use anki_proto::import_export::import_response::Log as NoteLog;

use crate::prelude::*;

impl crate::services::ImportExportService for Collection {
    fn import_anki_package(
        &mut self,
        input: anki_proto::import_export::ImportAnkiPackageRequest,
    ) -> Result<anki_proto::import_export::ImportResponse> {
        self.import_apkg(&input.package_path, input.options.unwrap_or_default())
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

#[cfg(test)]
mod tests {
    use anki_proto::import_export::import_response::Log as NoteLog;
    use anki_proto::import_export::CsvMetadataRequest;
    use anki_proto::import_export::ImportAnkiPackageRequest;
    use anki_proto::import_export::ImportCsvRequest;
    use tempfile::tempdir;

    use super::*;
    use crate::ops::Op;
    use crate::ops::OpChanges;
    use crate::ops::OpOutput;
    use crate::ops::StateChanges;
    use crate::services::ImportExportService;

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
