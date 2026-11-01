<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { goto } from "$app/navigation";
    import * as tr from "@generated/ftl";
    import { onMount } from "svelte";

    import UntrustedHtml from "$lib/components/UntrustedHtml.svelte";

    import { pendingNotice } from "../../review/lib";
    import CongratsPage from "../../congrats/CongratsPage.svelte";
    import type { PageData } from "./$types";

    export let data: PageData;

    const saved = data.saved;
    const notice = pendingNotice.text;
    pendingNotice.text = "";
    $: finished = data.queued.cards.length === 0;
    $: filtered = data.deck.kind.case === "filtered";
    $: optionsHref = `/deck-options/${data.did}`;

    function study(): Promise<void> {
        return goto("/review");
    }

    onMount(() => {
        if (saved) {
            history.replaceState(history.state, "", location.pathname);
        }
    });
</script>

<main class="overview">
    <h1>{data.deck.name}</h1>

    {#if saved}
        <p class="saved" role="status">Options saved.</p>
    {/if}

    {#if notice}
        <p class="saved" role="status">{notice}</p>
    {/if}

    {#if finished}
        <CongratsPage info={data.congrats} />
    {:else}
        <table class="counts">
            <tbody>
                <tr>
                    <td>{tr.actionsNew()}:</td>
                    <td class="new-count">{data.queued.newCount}</td>
                </tr>
                <tr>
                    <td>{tr.schedulingLearning()}:</td>
                    <td class="learn-count">{data.queued.learningCount}</td>
                </tr>
                <tr>
                    <td>{tr.studyingToReview()}:</td>
                    <td class="review-count">{data.queued.reviewCount}</td>
                </tr>
            </tbody>
        </table>
        <!-- svelte-ignore a11y-autofocus -->
        <button class="btn btn-primary study" autofocus on:click={study}>
            {tr.studyingStudyNow()}
        </button>
        {#if data.description}
            <div class="description"><UntrustedHtml html={data.description} /></div>
        {/if}
    {/if}

    <nav data-sveltekit-reload>
        {#if !filtered}
            <a href={optionsHref}>{tr.actionsOptions()}</a>
        {/if}
        <a href="/">{tr.actionsDecks()}</a>
    </nav>
</main>

<style lang="scss">
    .overview {
        display: flex;
        flex-direction: column;
        align-items: center;
        max-width: 44em;
        margin: 1.5em auto;
        padding: 0 1em;
    }

    h1 {
        font-size: 1.5em;
        font-weight: 600;
        overflow-wrap: anywhere;
    }

    .saved {
        color: var(--fg-subtle);
    }

    .counts td {
        padding: 0.2em 0.5em;

        &:last-child {
            font-weight: bold;
            text-align: end;
        }
    }

    .study {
        margin: 1em 0;
    }

    .description {
        width: 100%;
        margin-top: 1em;
    }

    nav {
        display: flex;
        gap: 1.5em;
        margin-top: 1.5em;

        a {
            color: var(--fg-link);
            text-decoration: none;
        }
    }
</style>
