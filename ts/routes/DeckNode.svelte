<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import type { DeckTreeNode } from "@generated/anki/decks_pb";
    import { SetDeckCollapsedRequest_Scope } from "@generated/anki/decks_pb";
    import { setDeckCollapsed } from "@generated/backend";

    export let node: DeckTreeNode;
    export let depth = 0;
    export let onOpen: (deckId: bigint) => void;

    $: hasChildren = node.children.length > 0;

    async function toggle(): Promise<void> {
        const collapsed = !node.collapsed;
        await setDeckCollapsed({
            deckId: node.deckId,
            collapsed,
            scope: SetDeckCollapsedRequest_Scope.REVIEWER,
        });
        node.collapsed = collapsed;
    }
</script>

<tr class="deck" data-deck={node.name}>
    <td class="name" style:padding-inline-start="{0.25 + depth * 1}em">
        <div class="name-inner">
            {#if hasChildren}
                <button
                    class="collapse"
                    aria-expanded={!node.collapsed}
                    aria-label="{node.collapsed ? 'Expand' : 'Collapse'} {node.name}"
                    on:click={toggle}
                >
                    {node.collapsed ? "▸" : "▾"}
                </button>
            {:else}
                <span class="collapse-spacer"></span>
            {/if}
            <a
                class:filtered={node.filtered}
                href="/overview/{node.deckId}"
                on:click|preventDefault={() => onOpen(node.deckId)}
            >
                {node.name}
            </a>
        </div>
    </td>
    <td class="count new-count" class:zero-count={!node.newCount}>{node.newCount}</td>
    <td class="count learn-count" class:zero-count={!node.learnCount}>
        {node.learnCount}
    </td>
    <td class="count review-count" class:zero-count={!node.reviewCount}>
        {node.reviewCount}
    </td>
    <td class="options">
        {#if !node.filtered}
            <a
                href="/deck-options/{node.deckId}"
                data-sveltekit-reload
                aria-label="Options for {node.name}"
                title="Options"
            >
                ⚙
            </a>
        {/if}
    </td>
</tr>
{#if hasChildren && !node.collapsed}
    {#each node.children as child (child.deckId)}
        <svelte:self node={child} depth={depth + 1} {onOpen} />
    {/each}
{/if}

<style lang="scss">
    .deck:hover {
        background: var(--canvas-inset);
    }

    td {
        padding: 0 8px;
    }

    .name {
        overflow-wrap: anywhere;

        .name-inner {
            display: flex;
            align-items: center;
        }

        a {
            flex: 1;
            display: flex;
            align-items: center;
            min-height: 44px;
            min-width: 0;
            color: var(--fg);
            text-decoration: none;

            &:hover {
                text-decoration: underline;
            }

            &.filtered {
                color: var(--fg-link);
            }
        }
    }

    .collapse,
    .collapse-spacer {
        flex: none;
        display: inline-block;
        width: 1.25em;
    }

    .collapse {
        padding: 0;
        border: 0;
        background: none;
        color: var(--fg-subtle);
        cursor: pointer;
        min-height: 44px;

        // a tall ring around a small arrow, after a tap or click, looks like a bug
        &:focus:not(:focus-visible) {
            outline: none;
        }
    }

    .count {
        text-align: end;
        font-variant-numeric: tabular-nums;
    }

    .options {
        text-align: center;
        padding: 0;

        a {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            min-width: 44px;
            min-height: 44px;
            color: var(--fg-subtle);
            text-decoration: none;

            &:hover {
                color: var(--fg);
            }
        }
    }

    @media (pointer: coarse), (max-width: 30em) {
        .collapse,
        .collapse-spacer {
            width: 2em;
        }

        td {
            padding: 0 5px;
        }
    }
</style>
