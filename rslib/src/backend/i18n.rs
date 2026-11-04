// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_proto::generic;
use anki_proto::i18n::I18nResourcesRequest;

use super::Backend;
use crate::prelude::*;

// Implemented here rather than on the collection, as tr doesn't require a
// collection lock.
impl crate::services::BackendI18nService for Backend {
    fn i18n_resources(&self, input: I18nResourcesRequest) -> Result<generic::Json> {
        serde_json::to_vec(&self.tr.resources_for_js(&input.modules))
            .map(Into::into)
            .map_err(Into::into)
    }
}
