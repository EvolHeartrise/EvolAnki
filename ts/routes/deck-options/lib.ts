// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import type { PlainMessage } from "@bufbuild/protobuf";
import type {
    DeckConfigsForUpdate,
    DeckConfigsForUpdate_CurrentDeck,
    UpdateDeckConfigsMode,
    UpdateDeckConfigsRequest,
} from "@generated/anki/deck_config_pb";
import type { DeckConfig, DeckConfig_Config } from "@generated/anki/deck_config_pb";
import { DeckConfigsForUpdate_CurrentDeck_Limits } from "@generated/anki/deck_config_pb";
import { updateDeckConfigs } from "@generated/backend";
import { runWithBackendProgress } from "@tslib/progress";
import { promiseWithResolver } from "@tslib/promise";
import { cloneDeep, isEqual, isEqualWith } from "lodash-es";
import { tick } from "svelte";
import type { Writable } from "svelte/store";
import { get, writable } from "svelte/store";

export type DeckOptionsId = bigint;

type AllConfigs =
    & Required<
        Pick<
            PlainMessage<UpdateDeckConfigsRequest>,
            | "limits"
            | "newCardsIgnoreReviewLimit"
            | "applyAllParentLimits"
            | "fsrsReschedule"
        >
    >
    & { config: DeckConfig; currentConfig: DeckConfig_Config };

export class DeckOptionsState {
    readonly currentConfig: Writable<DeckConfig_Config>;
    readonly currentDeck: DeckConfigsForUpdate_CurrentDeck;
    readonly deckLimits: Writable<DeckConfigsForUpdate_CurrentDeck_Limits>;
    readonly defaults: DeckConfig_Config;
    readonly newCardsIgnoreReviewLimit: Writable<boolean>;
    readonly applyAllParentLimits: Writable<boolean>;
    readonly fsrsReschedule: Writable<boolean> = writable(false);
    readonly fsrsHealthCheck: Writable<boolean>;
    readonly daysSinceLastOptimization: Writable<number>;
    /** Used to detect if there are any pending changes */
    originalConfigsPromise: Promise<AllConfigs>;
    readonly originalConfigsResolve: (value: AllConfigs) => void;

    readonly targetDeckId: DeckOptionsId;
    private config: DeckConfig;

    constructor(targetDeckId: DeckOptionsId, data: DeckConfigsForUpdate) {
        this.targetDeckId = targetDeckId;
        this.currentDeck = data.currentDeck!;
        this.defaults = data.defaults!.config!;
        this.config = data.config!;
        this.deckLimits = writable(data.currentDeck?.limits ?? createLimits());
        this.newCardsIgnoreReviewLimit = writable(data.newCardsIgnoreReviewLimit);
        this.applyAllParentLimits = writable(data.applyAllParentLimits);
        this.fsrsHealthCheck = writable(data.fsrsHealthCheck);
        this.daysSinceLastOptimization = writable(data.daysSinceLastFsrsOptimize);

        this.currentConfig = writable(cloneDeep(this.config.config!));

        // update our state when the current config is changed
        this.currentConfig.subscribe((val) => this.onCurrentConfigChanged(val));

        // Must be resolved after all components are mounted, as some components
        // may modify the config during their initialization.
        [this.originalConfigsPromise, this.originalConfigsResolve] = promiseWithResolver<AllConfigs>();
    }

    /** The search that shows what this deck's FSRS tools look at by default. */
    defaultSearch(): string {
        return this.currentDeck.defaultSearch;
    }

    dataForSaving(
        mode: UpdateDeckConfigsMode,
    ): PlainMessage<UpdateDeckConfigsRequest> {
        return {
            targetDeckId: this.targetDeckId,
            config: this.config,
            mode,
            limits: get(this.deckLimits),
            newCardsIgnoreReviewLimit: get(this.newCardsIgnoreReviewLimit),
            applyAllParentLimits: get(this.applyAllParentLimits),
            fsrsReschedule: get(this.fsrsReschedule),
            fsrsHealthCheck: get(this.fsrsHealthCheck),
        };
    }

    async save(mode: UpdateDeckConfigsMode): Promise<void> {
        await updateDeckConfigs(
            this.dataForSaving(mode),
        );
        // what's now saved is what later changes should be compared with
        this.originalConfigsPromise = Promise.resolve(cloneDeep(this.getAllConfigs()));
    }

    private onCurrentConfigChanged(config: DeckConfig_Config): void {
        if (!isEqual(config, this.config.config)) {
            this.config.config = config;
        }
    }

    private getAllConfigs(): AllConfigs {
        return cloneDeep({
            config: this.config,
            limits: get(this.deckLimits),
            newCardsIgnoreReviewLimit: get(this.newCardsIgnoreReviewLimit),
            applyAllParentLimits: get(this.applyAllParentLimits),
            fsrsReschedule: get(this.fsrsReschedule),
            currentConfig: get(this.currentConfig),
        });
    }

    async isModified(): Promise<boolean> {
        const original = await this.originalConfigsPromise;
        const current = this.getAllConfigs();
        return !isEqualWith(original, current, (lhs, rhs) => {
            if (typeof lhs === "number" && typeof rhs === "number") {
                // rslib hands us 32-bit floats (f32), while ts uses 64-bit floats
                // SpinBox and ParamsInput both round their values as f64 on blur
                // while the original config's corresponding value remains an f32
                // so we convert both to f32 before checking for equality
                return Math.fround(lhs) === Math.fround(rhs);
            }
            // undefined means fallback to isEqual
        });
    }

    resolveOriginalConfigs(): void {
        this.originalConfigsResolve(this.getAllConfigs());
    }
}

export function createLimits(): DeckConfigsForUpdate_CurrentDeck_Limits {
    return new DeckConfigsForUpdate_CurrentDeck_Limits({});
}

export class ValueTab {
    readonly title: string;
    value: number | null;
    private setter: (value: number | null) => void;
    private disabledValue: number | null;
    private startValue: number | null;
    private initialValue: number | null;

    constructor(
        title: string,
        value: number | null,
        setter: (value: number | null) => void,
        disabledValue: number | null,
        startValue: number | null,
    ) {
        this.title = title;
        this.value = this.initialValue = value;
        this.setter = setter;
        this.disabledValue = disabledValue;
        this.startValue = startValue;
    }

    reset(): void {
        this.setter(this.initialValue);
    }

    disable(): void {
        this.setter(this.disabledValue);
    }

    enable(fallbackValue: number): void {
        this.value = this.value ?? this.startValue ?? fallbackValue;
        this.setter(this.value);
    }

    setValue(value: number): void {
        this.value = value;
        this.setter(value);
    }
}

/** Set while a save is running; the current step's description, if any. */
export const saveInProgress = writable<{ label: string } | null>(null);

export async function saveWithProgress(
    state: DeckOptionsState,
    mode: UpdateDeckConfigsMode,
): Promise<void> {
    saveInProgress.set({ label: "" });
    try {
        await runWithBackendProgress(() => state.save(mode), (progress) => {
            const value = progress.value.value;
            saveInProgress.set({ label: typeof value === "string" ? value : "" });
        });
    } finally {
        saveInProgress.set(null);
    }
}

/** Ensure blur handler has fired so changes get committed. */
export async function commitEditing(): Promise<void> {
    if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
    }
    await tick();
}

export function fsrsParams(config: DeckConfig_Config): number[] {
    if (config.fsrsParams6) {
        return config.fsrsParams6;
    } else if (config.fsrsParams5) {
        return config.fsrsParams5;
    } else {
        return config.fsrsParams4;
    }
}
