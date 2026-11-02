// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/* eslint
@typescript-eslint/no-explicit-any: "off",
 */

import { protoBase64 } from "@bufbuild/protobuf";
import { DeckConfigsForUpdate, UpdateDeckConfigsMode } from "@generated/anki/deck_config_pb";
import { get } from "svelte/store";
import { expect, test, vi } from "vitest";

import { DeckOptionsState } from "./lib";

const exampleData = {
    config: {
        id: 123n,
        name: "123",
        mtimeSecs: 1618570764n,
        config: {
            learnSteps: [1, 10],
            relearnSteps: [10],
            newPerDay: 10,
            reviewsPerDay: 200,
            initialEase: 2.5,
            easyMultiplier: 1.2999999523162842,
            hardMultiplier: 1.2000000476837158,
            intervalMultiplier: 1,
            maximumReviewInterval: 36500,
            minimumLapseInterval: 1,
            graduatingIntervalGood: 1,
            graduatingIntervalEasy: 4,
            capAnswerTimeToSecs: 60,
            other: protoBase64.dec(
                "eyJuZXciOnsic2VwYXJhdGUiOnRydWV9LCJyZXYiOnsiZnV6eiI6MC4wNSwibWluU3BhY2UiOjF9fQ==",
            ),
        },
    },
    currentDeck: {
        name: "Default::child",
        defaultSearch: "(\"deck:Default::child\" -is:suspended)",
    },
    defaults: {
        config: {
            learnSteps: [1, 10],
            relearnSteps: [10],
            newPerDay: 20,
            reviewsPerDay: 200,
            initialEase: 2.5,
            easyMultiplier: 1.2999999523162842,
            hardMultiplier: 1.2000000476837158,
            intervalMultiplier: 1,
            maximumReviewInterval: 36500,
            minimumLapseInterval: 1,
            graduatingIntervalGood: 1,
            graduatingIntervalEasy: 4,
            capAnswerTimeToSecs: 60,
        },
    },
};

function startingState(): DeckOptionsState {
    return new DeckOptionsState(
        123n,
        new DeckConfigsForUpdate(exampleData),
    );
}

test("start", () => {
    const state = startingState();
    expect(state.currentDeck.name).toBe("Default::child");
    expect(get(state.currentConfig).newPerDay).toBe(10);
});

test("the default search comes from the deck", () => {
    expect(startingState().defaultSearch()).toBe("(\"deck:Default::child\" -is:suspended)");
});

test("saving sends the deck's settings", () => {
    const state = startingState();
    let out = state.dataForSaving(UpdateDeckConfigsMode.NORMAL);
    expect(out.targetDeckId).toBe(123n);
    expect(out.config!.config!.newPerDay).toBe(10);
    expect(out.mode).toBe(UpdateDeckConfigsMode.NORMAL);

    const config = get(state.currentConfig);
    config.newPerDay = 5;
    state.currentConfig.set(config);
    out = state.dataForSaving(UpdateDeckConfigsMode.COMPUTE_ALL_PARAMS);
    expect(out.config!.config!.newPerDay).toBe(5);
    expect(out.mode).toBe(UpdateDeckConfigsMode.COMPUTE_ALL_PARAMS);
});

test("a save becomes the baseline for later changes", async () => {
    const fetchMock = vi.fn(async () => new Response(new Uint8Array(), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    try {
        const state = startingState();
        state.resolveOriginalConfigs();
        const config = get(state.currentConfig);
        config.newPerDay += 1;
        state.currentConfig.set(config);
        expect(await state.isModified()).toBe(true);

        await state.save(UpdateDeckConfigsMode.NORMAL);
        expect(fetchMock).toHaveBeenCalledOnce();
        expect(await state.isModified()).toBe(false);

        config.newPerDay += 1;
        state.currentConfig.set(config);
        expect(await state.isModified()).toBe(true);
    } finally {
        vi.unstubAllGlobals();
    }
});
