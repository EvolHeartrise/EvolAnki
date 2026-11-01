// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { ConfigKey_Bool } from "@generated/anki/config_pb";
import { getConfigBool, getCurrentDeck } from "@generated/backend";
import { getFrameBase } from "@tslib/webapp";

import type { PageLoad } from "./$types";

export const load = (async () => {
    const [frameBase, deck, hidePlayButtons] = await Promise.all([
        getFrameBase(),
        getCurrentDeck({}),
        getConfigBool({ key: ConfigKey_Bool.HIDE_AUDIO_PLAY_BUTTONS }),
    ]);
    return { frameBase, deckId: deck.id, hidePlayButtons: hidePlayButtons.val };
}) satisfies PageLoad;
