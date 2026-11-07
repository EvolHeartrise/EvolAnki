// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/* eslint
@typescript-eslint/no-explicit-any: "off",
 */

import type { GraphsResponse } from "@generated/anki/stats_pb";
import { dayLabel } from "@tslib/time";
import * as tr from "@tslib/tr";
import type { Bin } from "d3";
import { bin, interpolateBlues, min, scaleLinear, scaleSequential, sum } from "d3";

import type { TableDatum } from "./graph-helpers";
import { getNumericMapBinValue, GraphRange, numericMap } from "./graph-helpers";
import type { HistogramData } from "./histogram-graph";

export interface GraphData {
    daysAdded: Map<number, number>;
}

export function gatherData(data: GraphsResponse): GraphData {
    return { daysAdded: numericMap(data.added!.added) };
}

export function buildHistogram(
    data: GraphData,
    range: GraphRange,
): [HistogramData | null, TableDatum[]] {
    // get min/max
    const total = data.daysAdded.size;
    if (!total) {
        return [null, []];
    }

    let xMin: number;

    // cap max to selected range
    switch (range) {
        case GraphRange.Month:
            xMin = -31;
            break;
        case GraphRange.ThreeMonths:
            xMin = -90;
            break;
        case GraphRange.Year:
            xMin = -365;
            break;
        case GraphRange.AllTime:
            xMin = min(data.daysAdded.keys())!;
            break;
    }
    const xMax = 1;
    const desiredBars = Math.min(70, Math.abs(xMin!));

    const scale = scaleLinear().domain([xMin!, xMax]);
    const bins = bin()
        .value((m) => {
            return m[0];
        })
        .domain(scale.domain() as any)
        .thresholds(scale.ticks(desiredBars))(data.daysAdded.entries() as any);

    // empty graph?
    const accessor = getNumericMapBinValue as any;
    if (!sum(bins, accessor)) {
        return [null, []];
    }

    const adjustedRange = scaleLinear().range([0.7, 0.3]);
    const colourScale = scaleSequential((n) => interpolateBlues(adjustedRange(n)!)).domain([xMax!, xMin!]);

    const totalInPeriod = sum(bins, accessor);
    const periodDays = Math.abs(xMin!);
    const cardsPerDay = Math.round(totalInPeriod / periodDays);
    const tableData = [
        {
            label: tr.statisticsTotal(),
            value: tr.statisticsCards({ cards: totalInPeriod }),
        },
        {
            label: tr.statisticsAverage(),
            value: tr.statisticsCardsPerDay({ count: cardsPerDay }),
        },
    ];

    function hoverText(
        bin: Bin<number, number>,
        cumulative: number,
        _percent: number,
    ): string {
        const day = dayLabel(bin.x0!, bin.x1!);
        const cards = tr.statisticsCards({ cards: accessor(bin) });
        const total = tr.statisticsRunningTotal();
        const totalCards = tr.statisticsCards({ cards: cumulative });
        return `${day}:<br>${cards}<br>${total}: ${totalCards}`;
    }

    return [
        {
            scale,
            bins,
            total: totalInPeriod,
            hoverText,
            colourScale,
            binValue: getNumericMapBinValue,
            showArea: true,
        },
        tableData,
    ];
}
