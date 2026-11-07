<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import Container from "$lib/components/Container.svelte";
    import Row from "$lib/components/Row.svelte";

    import AdvancedOptions from "./AdvancedOptions.svelte";
    import AudioOptions from "./AudioOptions.svelte";
    import DailyLimits from "./DailyLimits.svelte";
    import DeckOptionsHeader from "./DeckOptionsHeader.svelte";
    import DisplayOrder from "./DisplayOrder.svelte";
    import FsrsOptionsOuter from "./FsrsOptionsOuter.svelte";
    import LapseOptions from "./LapseOptions.svelte";
    import { type DeckOptionsState, saveInProgress } from "./lib";
    import NewOptions from "./NewOptions.svelte";
    import TimerOptions from "./TimerOptions.svelte";

    export let state: DeckOptionsState;
</script>

<DeckOptionsHeader {state} />

<div class="deck-options-page">
    <Container
        breakpoint="sm"
        --gutter-inline="0.25rem"
        --gutter-block="0.75rem"
        class="container-columns"
    >
        <div>
            <Row class="row-columns">
                <DailyLimits {state} />
            </Row>

            <Row class="row-columns">
                <NewOptions {state} />
            </Row>

            <Row class="row-columns">
                <LapseOptions {state} />
            </Row>

            <Row class="row-columns">
                <DisplayOrder {state} />
            </Row>

            <Row class="row-columns">
                <FsrsOptionsOuter {state} />
            </Row>
        </div>

        <div>
            <Row class="row-columns">
                <AudioOptions {state} />
            </Row>

            <Row class="row-columns">
                <TimerOptions {state} />
            </Row>

            <Row class="row-columns">
                <AdvancedOptions {state} />
            </Row>
        </div>
    </Container>
</div>

{#if $saveInProgress}
    <div class="saving" role="status">
        <div>Saving…</div>
        {#if $saveInProgress.label}
            <div>{$saveInProgress.label}</div>
        {/if}
    </div>
{/if}

<style lang="scss">
    @use "$lib/sass/breakpoints" as bp;

    .deck-options-page {
        overflow-x: hidden;
        word-break: break-word;

        :global(.container-columns) {
            display: grid;
            gap: 0px;
        }

        @include bp.with-breakpoint("lg") {
            :global(.container-columns) {
                grid-template-columns: repeat(2, 1fr);
                gap: 20px;
            }
        }
    }

    .saving {
        position: fixed;
        inset: 0;
        z-index: 100;
        display: flex;
        flex-direction: column;
        gap: 0.5em;
        align-items: center;
        justify-content: center;
        color: var(--fg);
        background: color-mix(in srgb, var(--canvas) 80%, transparent);
    }
</style>
