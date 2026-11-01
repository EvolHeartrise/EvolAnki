<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { goto } from "$app/navigation";
    import type { DeckTreeNode } from "@generated/anki/decks_pb";
    import { getCurrentDeck, setCurrentDeck } from "@generated/backend";
    import * as tr from "@generated/ftl";

    import { pendingNotice } from "./review/lib";
    import DeckNode from "./DeckNode.svelte";
    import type { PageData } from "./$types";

    export let data: PageData;

    const notice = pendingNotice.text;
    pendingNotice.text = "";

    /** Like Qt, leave out the default deck when it's empty and others exist. */
    function shown(root: DeckTreeNode): DeckTreeNode[] {
        return root.children.filter(
            (node) =>
                !(
                    node.deckId === 1n &&
                    root.children.length > 1 &&
                    !node.children.length &&
                    !node.totalIncludingChildren
                ),
        );
    }

    async function open(deckId: bigint): Promise<void> {
        // selecting a deck is an undoable step, so don't add one needlessly
        if ((await getCurrentDeck({})).id !== deckId) {
            await setCurrentDeck({ did: deckId });
        }
        await goto(`/overview/${deckId}`);
    }
</script>

<main class="decks">
    {#if notice}
        <p class="notice" role="status">{notice}</p>
    {/if}
    <table>
        <thead>
            <tr>
                <th class="name">{tr.decksDeck()}</th>
                <th>{tr.actionsNew()}</th>
                <th>{tr.schedulingLearning()}</th>
                <th>{tr.statisticsDueCount()}</th>
                <th><span class="visually-hidden">{tr.actionsOptions()}</span></th>
            </tr>
        </thead>
        <tbody>
            {#each shown(data.tree) as node (node.deckId)}
                <DeckNode {node} onOpen={open} />
            {/each}
        </tbody>
    </table>
    <p class="studied">{data.studied}</p>
</main>

<style lang="scss">
    .decks {
        max-width: 44em;
        margin: 1.5em auto;
        padding: 0 1em;
        padding-left: max(1em, env(safe-area-inset-left));
        padding-right: max(1em, env(safe-area-inset-right));
        padding-bottom: env(safe-area-inset-bottom);
    }

    .notice {
        color: var(--fg-subtle);
    }

    @media (pointer: coarse) {
        th {
            padding-inline: 5px;
        }
    }

    @media (max-width: 30em) {
        .decks {
            margin-top: 0.5em;
            padding-left: max(0.25em, env(safe-area-inset-left));
            padding-right: max(0.25em, env(safe-area-inset-right));
        }

        th {
            font-size: 0.85em;
            padding-inline: 5px;
        }
    }

    table {
        width: 100%;
        border-collapse: collapse;
    }

    th {
        padding: 0.4em 8px;
        font-weight: 600;
        text-align: end;
        white-space: nowrap;
        border-bottom: 1px solid var(--border-subtle);

        &.name {
            text-align: start;
            width: 100%;
        }
    }

    .studied {
        margin-top: 1.5em;
        text-align: center;
        color: var(--fg-subtle);
    }
</style>
