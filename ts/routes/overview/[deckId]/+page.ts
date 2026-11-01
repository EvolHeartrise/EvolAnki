// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { getCurrentDeck, getDeck, getQueuedCards, renderMarkdown, setCurrentDeck } from "@generated/backend";

import type { PageLoad } from "./$types";

export const load = (async ({ params, url }) => {
    const did = BigInt(params.deckId);
    // the page can be reached by a link, and everything below uses the current deck
    if ((await getCurrentDeck({})).id !== did) {
        await setCurrentDeck({ did });
    }
    const [deck, queued] = await Promise.all([
        getDeck({ did }),
        getQueuedCards({ fetchLimit: 1, intradayLearningOnly: false }),
    ]);
    let description = "";
    if (deck.kind.case === "normal") {
        description = deck.kind.value.description;
        if (description && deck.kind.value.markdownDescription) {
            const rendered = await renderMarkdown({ markdown: description, sanitize: true });
            description = rendered.val;
        }
    }
    return {
        did,
        deck,
        queued,
        description,
        saved: url.searchParams.has("saved"),
    };
}) satisfies PageLoad;
