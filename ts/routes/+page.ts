// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { deckTree, studiedToday } from "@generated/backend";

import type { PageLoad } from "./$types";

export const load = (async () => {
    const [tree, studied] = await Promise.all([
        deckTree({ now: BigInt(Math.floor(Date.now() / 1000)) }),
        studiedToday({}),
    ]);
    return { tree, studied: studied.val };
}) satisfies PageLoad;
