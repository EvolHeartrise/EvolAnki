// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_proto::config::config_key::Bool as BoolKeyProto;
use anki_proto::generic;

use crate::config::BoolKey;
use crate::prelude::*;

impl From<BoolKeyProto> for BoolKey {
    fn from(k: BoolKeyProto) -> Self {
        match k {
            BoolKeyProto::HideAudioPlayButtons => BoolKey::HideAudioPlayButtons,
        }
    }
}

impl crate::services::ConfigService for Collection {
    fn get_config_bool(
        &mut self,
        input: anki_proto::config::GetConfigBoolRequest,
    ) -> Result<generic::Bool> {
        Ok(generic::Bool {
            val: Collection::get_config_bool(self, input.key().into()),
        })
    }
}
