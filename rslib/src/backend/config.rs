// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_proto::config::config_key::Bool as BoolKeyProto;
use anki_proto::config::config_key::String as StringKeyProto;
use anki_proto::generic;

use crate::config::BoolKey;
use crate::config::StringKey;
use crate::prelude::*;

impl From<BoolKeyProto> for BoolKey {
    fn from(k: BoolKeyProto) -> Self {
        match k {
            BoolKeyProto::BrowserTableShowNotesMode => BoolKey::BrowserTableShowNotesMode,
            BoolKeyProto::PreviewBothSides => BoolKey::PreviewBothSides,
            BoolKeyProto::CollapseTags => BoolKey::CollapseTags,
            BoolKeyProto::CollapseNotetypes => BoolKey::CollapseNotetypes,
            BoolKeyProto::CollapseDecks => BoolKey::CollapseDecks,
            BoolKeyProto::CollapseSavedSearches => BoolKey::CollapseSavedSearches,
            BoolKeyProto::CollapseToday => BoolKey::CollapseToday,
            BoolKeyProto::CollapseCardState => BoolKey::CollapseCardState,
            BoolKeyProto::Sched2021 => BoolKey::Sched2021,
            BoolKeyProto::AddingDefaultsToCurrentDeck => BoolKey::AddingDefaultsToCurrentDeck,
            BoolKeyProto::HideAudioPlayButtons => BoolKey::HideAudioPlayButtons,
            BoolKeyProto::InterruptAudioWhenAnswering => BoolKey::InterruptAudioWhenAnswering,
            BoolKeyProto::PasteImagesAsPng => BoolKey::PasteImagesAsPng,
            BoolKeyProto::PasteStripsFormatting => BoolKey::PasteStripsFormatting,
            BoolKeyProto::NormalizeNoteText => BoolKey::NormalizeNoteText,
            BoolKeyProto::IgnoreAccentsInSearch => BoolKey::IgnoreAccentsInSearch,
            BoolKeyProto::RestorePositionBrowser => BoolKey::RestorePositionBrowser,
            BoolKeyProto::RestorePositionReviewer => BoolKey::RestorePositionReviewer,
            BoolKeyProto::ResetCountsBrowser => BoolKey::ResetCountsBrowser,
            BoolKeyProto::ResetCountsReviewer => BoolKey::ResetCountsReviewer,
            BoolKeyProto::RandomOrderReposition => BoolKey::RandomOrderReposition,
            BoolKeyProto::ShiftPositionOfExistingCards => BoolKey::ShiftPositionOfExistingCards,
            BoolKeyProto::RenderLatex => BoolKey::RenderLatex,
            BoolKeyProto::LoadBalancerEnabled => BoolKey::LoadBalancerEnabled,
            BoolKeyProto::FsrsShortTermWithStepsEnabled => BoolKey::FsrsShortTermWithStepsEnabled,
            BoolKeyProto::FsrsLegacyEvaluate => BoolKey::FsrsLegacyEvaluate,
        }
    }
}

impl From<StringKeyProto> for StringKey {
    fn from(k: StringKeyProto) -> Self {
        match k {
            StringKeyProto::SetDueBrowser => StringKey::SetDueBrowser,
            StringKeyProto::SetDueReviewer => StringKey::SetDueReviewer,
            StringKeyProto::DefaultSearchText => StringKey::DefaultSearchText,
            StringKeyProto::CardStateCustomizer => StringKey::CardStateCustomizer,
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
