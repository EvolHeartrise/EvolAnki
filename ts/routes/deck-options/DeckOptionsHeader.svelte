<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import StickyContainer from "$lib/components/StickyContainer.svelte";

    import type { DeckOptionsState } from "./lib";
    import SaveButton from "./SaveButton.svelte";

    export let state: DeckOptionsState;
</script>

<StickyContainer --gutter-block="0.5rem" --sticky-borders="0 0 1px" breakpoint="sm">
    <div class="button-bar">
        <h1 class="deck-name">{state.currentDeck.name}</h1>
        <SaveButton {state} />
    </div>
</StickyContainer>

<style lang="scss">
    .button-bar {
        display: flex;
        flex-wrap: nowrap;
        justify-content: space-between;
        align-items: center;
        gap: 1rem;

        flex-grow: 1;
    }

    .deck-name {
        margin: 0;
        min-width: 0;
        font-size: 1.1rem;
        overflow-wrap: anywhere;
    }
</style>
