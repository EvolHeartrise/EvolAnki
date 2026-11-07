<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import type { GraphsResponse } from "@generated/anki/stats_pb";
    import {
        getGraphPreferences,
        graphs,
        setGraphPreferences,
    } from "@generated/backend";
    import { writable } from "svelte/store";

    import { autoSavingPrefs } from "$lib/sveltelib/preferences";
    import { pageTheme } from "$lib/sveltelib/theme";

    import AddedGraph from "./AddedGraph.svelte";
    import ButtonsGraph from "./ButtonsGraph.svelte";
    import CalendarGraph from "./CalendarGraph.svelte";
    import CardCounts from "./CardCounts.svelte";
    import DifficultyGraph from "./DifficultyGraph.svelte";
    import FutureDue from "./FutureDue.svelte";
    import { daysToRevlogRange } from "./graph-helpers";
    import HourGraph from "./HourGraph.svelte";
    import IntervalsGraph from "./IntervalsGraph.svelte";
    import RangeBox from "./RangeBox.svelte";
    import RetrievabilityGraph from "./RetrievabilityGraph.svelte";
    import ReviewsGraph from "./ReviewsGraph.svelte";
    import StabilityGraph from "./StabilityGraph.svelte";
    import TodayStats from "./TodayStats.svelte";
    import TrueRetention from "./TrueRetention.svelte";

    const search = writable("deck:current");
    const days = writable(365);

    const prefsPromise = autoSavingPrefs(
        () => getGraphPreferences({}),
        setGraphPreferences,
    );

    let sourceData: GraphsResponse | null = null;
    let loading = true;
    $: updateSourceData($search, $days);

    async function updateSourceData(search: string, days: number): Promise<void> {
        // ensure the fast-loading preferences come first
        await prefsPromise;
        loading = true;
        try {
            sourceData = await graphs({ search, days });
        } finally {
            loading = false;
        }
    }

    $: revlogRange = daysToRevlogRange($days);
</script>

<!--
We block graphs loading until the preferences have been fetched, so graphs
don't have to worry about a null initial value. We don't do the same for the
graph data, as it gets updated as the user changes options, and we don't want
the current graphs to disappear until the new graphs have finished loading.
-->
{#await prefsPromise then prefs}
    <RangeBox {search} {days} {loading} />

    <div class="graphs-container">
        {#if sourceData && revlogRange}
            <TodayStats {sourceData} />
            <FutureDue {sourceData} {prefs} />
            <CalendarGraph
                {sourceData}
                {prefs}
                {revlogRange}
                nightMode={$pageTheme.isDark}
            />
            <ReviewsGraph {sourceData} {revlogRange} />
            <CardCounts {sourceData} {prefs} />
            <IntervalsGraph {sourceData} />
            <StabilityGraph {sourceData} />
            <DifficultyGraph {sourceData} />
            <RetrievabilityGraph {sourceData} />
            <TrueRetention {sourceData} {revlogRange} />
            <HourGraph {sourceData} {revlogRange} />
            <ButtonsGraph {sourceData} {revlogRange} />
            <AddedGraph {sourceData} />
        {/if}
    </div>
    <div class="spacer"></div>
{/await}

<style lang="scss">
    .graphs-container {
        display: grid;
        gap: 1em;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        // required on Safari to stretch whole width
        width: calc(100vw - 3em);
        margin-left: 1em;
        margin-right: 1em;

        @media only screen and (max-width: 600px) {
            width: calc(100vw - 1rem);
            margin-left: 0.5rem;
            margin-right: 0.5rem;
        }

        @media only screen and (max-width: 1400px) {
            grid-template-columns: 1fr 1fr;
        }
        @media only screen and (max-width: 1200px) {
            grid-template-columns: 1fr;
        }
        @media only screen and (max-width: 600px) {
            font-size: 12px;
        }

        @media only print {
            // grid layout does not honor page-break-inside
            display: block;
            margin-top: 3em;
        }
    }

    .spacer {
        height: 1.5em;
    }
</style>
