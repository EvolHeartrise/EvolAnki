<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import type { GraphsResponse } from "@generated/anki/stats_pb";
    import * as tr from "@tslib/tr";

    import Graph from "./Graph.svelte";
    import type { TableDatum } from "./graph-helpers";
    import type { HistogramData } from "./histogram-graph";
    import HistogramGraph from "./HistogramGraph.svelte";
    import { gatherData, prepareData } from "./retrievability";
    import TableData from "./TableData.svelte";
    import PercentageRange from "./PercentageRange.svelte";
    import { PercentageRangeEnum, PercentageRangeToQuantile } from "./percentageRange";

    export let sourceData: GraphsResponse | null = null;

    let histogramData: HistogramData | null = null;
    let tableData: TableDatum[] = [];
    let range = PercentageRangeEnum.All;

    $: if (sourceData) {
        [histogramData, tableData] = prepareData(
            gatherData(sourceData),
            PercentageRangeToQuantile(range),
        );
    }

    const title = tr.statisticsCardRetrievabilityTitle();
    const subtitle = tr.statisticsRetrievabilitySubtitle();
</script>

{#if sourceData}
    <Graph {title} {subtitle}>
        <PercentageRange bind:range />

        <HistogramGraph data={histogramData} />

        <TableData {tableData} />
    </Graph>
{/if}
