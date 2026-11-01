<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import * as tr from "@generated/ftl";
    import { HelpPage } from "@tslib/help-page";
    import type Carousel from "bootstrap/js/dist/carousel";
    import type Modal from "bootstrap/js/dist/modal";
    import HelpModal from "$lib/components/HelpModal.svelte";
    import SettingTitle from "$lib/components/SettingTitle.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";
    import type { HelpItem } from "$lib/components/types";

    import type { DeckOptionsState } from "./lib";
    import SpinBoxFloatRow from "./SpinBoxFloatRow.svelte";
    import SpinBoxRow from "./SpinBoxRow.svelte";
    import DateInput from "./DateInput.svelte";
    import Warning from "./Warning.svelte";
    import { getIgnoredBeforeCount } from "@generated/backend";
    import type { GetIgnoredBeforeCountResponse } from "@generated/anki/deck_config_pb";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const defaults = state.defaults;

    const settings = {
        maximumInterval: {
            title: tr.schedulingMaximumInterval(),
            help: tr.deckConfigMaximumIntervalTooltip(),
            url: HelpPage.DeckOptions.maximumInterval,
        },
        historicalRetention: {
            title: tr.deckConfigHistoricalRetention(),
            help: tr.deckConfigHistoricalRetentionTooltip(),
        },
        ignoreRevlogsBeforeMs: {
            title: tr.deckConfigIgnoreBefore(),
            help: tr.deckConfigIgnoreBeforeTooltip2(),
        },
    };
    const helpSections: HelpItem[] = Object.values(settings);

    $: maxIntervalWarningClass =
        $config.maximumReviewInterval < 50 ? "alert-danger" : "alert-warning";
    $: maxIntervalWarning =
        $config.maximumReviewInterval < 180
            ? tr.deckConfigTooShortMaximumInterval()
            : "";

    let ignoreRevlogsBeforeCount: GetIgnoredBeforeCountResponse | null = null;
    let lastIgnoreRevlogsBeforeDate = "";
    function updateIgnoreRevlogsBeforeCount(ignoreRevlogsBeforeDate: string) {
        if (lastIgnoreRevlogsBeforeDate == ignoreRevlogsBeforeDate) {
            return;
        }
        if (
            cutoffUpdatedSinceLoad &&
            ignoreRevlogsBeforeDate &&
            ignoreRevlogsBeforeDate != "1970-01-01"
        ) {
            lastIgnoreRevlogsBeforeDate = ignoreRevlogsBeforeDate;
            getIgnoredBeforeCount({
                search: $config.paramSearch || state.defaultSearch(),
                ignoreRevlogsBeforeDate,
            }).then((resp) => {
                ignoreRevlogsBeforeCount = resp;
            });
        } else {
            ignoreRevlogsBeforeCount = null;
        }
        cutoffUpdatedSinceLoad = true;
    }

    let timeoutId: ReturnType<typeof setTimeout> | undefined = undefined;
    // Running the card count check on startup is inefficient. After users have had a few months
    // to notice + update (e.g. from ~Oct 2025), we should change this to false.
    let cutoffUpdatedSinceLoad = true;
    const IGNORE_REVLOG_COUNT_DELAY_MS = 1000;

    $: {
        clearTimeout(timeoutId);
        timeoutId = setTimeout(() => {
            updateIgnoreRevlogsBeforeCount($config.ignoreRevlogsBeforeDate);
        }, IGNORE_REVLOG_COUNT_DELAY_MS);
    }
    let ignoreRevlogsBeforeWarningClass = "alert-warning";
    $: if (ignoreRevlogsBeforeCount) {
        // If there is less than a tenth of reviews included
        if (
            Number(ignoreRevlogsBeforeCount.included) /
                Number(ignoreRevlogsBeforeCount.total) <
            0.1
        ) {
            ignoreRevlogsBeforeWarningClass = "alert-danger";
        } else if (
            ignoreRevlogsBeforeCount.included != ignoreRevlogsBeforeCount.total
        ) {
            ignoreRevlogsBeforeWarningClass = "alert-warning";
        } else {
            ignoreRevlogsBeforeWarningClass = "alert-info";
        }
    }
    $: ignoreRevlogsBeforeWarning = ignoreRevlogsBeforeCount
        ? tr.deckConfigIgnoreBeforeInfo({
              included: ignoreRevlogsBeforeCount.included.toString(),
              totalCards: ignoreRevlogsBeforeCount.total.toString(),
          })
        : "";

    let modal: Modal;
    let carousel: Carousel;

    function openHelpModal(index: number): void {
        modal.show();
        carousel.to(index);
    }
</script>

<TitledContainer title={tr.deckConfigAdvancedTitle()}>
    <HelpModal
        title={tr.deckConfigAdvancedTitle()}
        url={HelpPage.DeckOptions.advanced}
        slot="tooltip"
        {helpSections}
        on:mount={(e) => {
            modal = e.detail.modal;
            carousel = e.detail.carousel;
        }}
    />
    <SpinBoxRow
        bind:value={$config.maximumReviewInterval}
        defaultValue={defaults.maximumReviewInterval}
        min={1}
        max={365 * 100}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("maximumInterval"))}
        >
            {settings.maximumInterval.title}
        </SettingTitle>
    </SpinBoxRow>
    <Warning warning={maxIntervalWarning} className={maxIntervalWarningClass}></Warning>

    <SpinBoxFloatRow
        bind:value={$config.historicalRetention}
        defaultValue={defaults.historicalRetention}
        min={0.5}
        max={1.0}
        percentage={true}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("historicalRetention"))}
        >
            {tr.deckConfigHistoricalRetention()}
        </SettingTitle>
    </SpinBoxFloatRow>
    <DateInput
        bind:date={$config.ignoreRevlogsBeforeDate}
        max={new Date().toLocaleDateString("en-CA")}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("ignoreRevlogsBeforeMs"))}
        >
            {tr.deckConfigIgnoreBefore()}
        </SettingTitle>
    </DateInput>
    <Warning
        warning={ignoreRevlogsBeforeWarning}
        className={ignoreRevlogsBeforeWarningClass}
    ></Warning>
</TitledContainer>
