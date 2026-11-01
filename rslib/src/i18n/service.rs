// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_i18n::I18n;
use anki_proto::generic;
use anki_proto::generic::Json;
use anki_proto::i18n::I18nResourcesRequest;

use crate::collection::Collection;
use crate::error;

impl crate::services::I18nService for Collection {
    fn i18n_resources(&mut self, input: I18nResourcesRequest) -> error::Result<Json> {
        i18n_resources(&self.tr, input)
    }
}

pub(crate) fn i18n_resources(
    tr: &I18n,
    input: I18nResourcesRequest,
) -> error::Result<generic::Json> {
    serde_json::to_vec(&tr.resources_for_js(&input.modules))
        .map(Into::into)
        .map_err(Into::into)
}
