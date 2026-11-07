<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import type { GraphsResponse } from "@generated/anki/stats_pb";
    import * as tr from "@tslib/tr";

    import type { GraphData } from "./added";
    import { buildHistogram, gatherData } from "./added";
    import Graph from "./Graph.svelte";
    import type { TableDatum } from "./graph-helpers";
    import { GraphRange, RevlogRange } from "./graph-helpers";
    import GraphRangeRadios from "./GraphRangeRadios.svelte";
    import type { HistogramData } from "./histogram-graph";
    import HistogramGraph from "./HistogramGraph.svelte";
    import InputBox from "./InputBox.svelte";
    import TableData from "./TableData.svelte";

    export let sourceData: GraphsResponse | null = null;

    let histogramData: HistogramData | null = null;
    let tableData: TableDatum[] = [];
    let graphRange: GraphRange = GraphRange.Month;

    let addedData: GraphData | null = null;
    $: if (sourceData) {
        addedData = gatherData(sourceData);
    }

    $: if (addedData) {
        [histogramData, tableData] = buildHistogram(addedData, graphRange);
    }

    const title = tr.statisticsAddedTitle();
    const subtitle = tr.statisticsAddedSubtitle();
</script>

<Graph {title} {subtitle}>
    <InputBox>
        <GraphRangeRadios bind:graphRange revlogRange={RevlogRange.All} />
    </InputBox>

    <HistogramGraph data={histogramData} />

    <TableData {tableData} />
</Graph>
