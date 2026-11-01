<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import type { CongratsInfoResponse } from "@generated/anki/scheduler_pb";
    import { congratsInfo } from "@generated/backend";
    import * as tr from "@generated/ftl";

    import Col from "$lib/components/Col.svelte";
    import Container from "$lib/components/Container.svelte";
    import UntrustedHtml from "$lib/components/UntrustedHtml.svelte";

    import { buildNextLearnMsg } from "./lib";
    import { onMount } from "svelte";

    export let info: CongratsInfoResponse;
    export let refreshPeriodically = true;

    const congrats = tr.schedulingCongratulationsFinished();
    let nextLearnMsg: string;
    $: nextLearnMsg = buildNextLearnMsg(info);
    const today_reviews = tr.schedulingTodayReviewLimitReached();
    const today_new = tr.schedulingTodayNewLimitReached();

    onMount(() => {
        if (refreshPeriodically) {
            setInterval(async () => {
                try {
                    info = await congratsInfo({}, { alertOnError: false });
                } catch {
                    console.log("congrats fetch failed");
                }
            }, 60000);
        }
    });
</script>

<Container --gutter-block="1rem" --gutter-inline="2px" breakpoint="sm">
    <Col --col-justify="center">
        <div class="congrats">
            <h1>{congrats}</h1>

            <p>{nextLearnMsg}</p>

            {#if info.reviewRemaining}
                <p>{today_reviews}</p>
            {/if}

            {#if info.newRemaining}
                <p>{today_new}</p>
            {/if}

            {#if info.haveSchedBuried || info.haveUserBuried}
                <p class="buried">
                    Some related cards are held back until tomorrow, so that you don't
                    see them on the same day.
                </p>
            {/if}

            <slot />

            {#if info.deckDescription}
                <div class="description">
                    {#if webApp}
                        <UntrustedHtml html={info.deckDescription} />
                    {:else}
                        {@html info.deckDescription}
                    {/if}
                </div>
            {/if}
        </div>
    </Col>
</Container>

<style lang="scss">
    .congrats {
        margin-top: 2em;
        max-width: 30em;
        font-size: var(--font-size);

        :global(a) {
            color: var(--fg-link);
            text-decoration: none;
        }
    }

    .description {
        border: 1px solid var(--border);
        padding: 1em;
    }
</style>
