<!--
    Copyright: Ankitects Pty Ltd and contributors
    License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import * as tr from "@tslib/tr";
    import { HelpPage } from "@tslib/help-page";
    import HelpModal from "$lib/components/HelpModal.svelte";
    import SettingTitle from "$lib/components/SettingTitle.svelte";
    import SwitchRow from "$lib/components/SwitchRow.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";

    import GlobalLabel from "./GlobalLabel.svelte";
    import type { DeckOptionsState } from "./lib";
    import { ValueTab } from "./lib";
    import SpinBoxRow from "./SpinBoxRow.svelte";
    import TabbedValue from "./TabbedValue.svelte";
    import Warning from "./Warning.svelte";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const limits = state.deckLimits;
    const defaults = state.defaults;
    const newCardsIgnoreReviewLimit = state.newCardsIgnoreReviewLimit;
    const applyAllParentLimits = state.applyAllParentLimits;

    const v3Extra =
        "\n\n" + tr.deckConfigLimitDeckV3() + "\n\n" + tr.deckConfigTabDescription();
    const reviewV3Extra = "\n\n" + tr.deckConfigLimitInterdayBoundByReviews() + v3Extra;
    const newCardsIgnoreReviewLimitHelp =
        tr.deckConfigAffectsEntireCollection() +
        "\n\n" +
        tr.deckConfigNewCardsIgnoreReviewLimitTooltip();
    const applyAllParentLimitsHelp =
        tr.deckConfigAffectsEntireCollection() +
        "\n\n" +
        tr.deckConfigApplyAllParentLimitsTooltip();

    $: reviewsTooLow =
        Math.min(9999, newValue * 10) > reviewsValue
            ? tr.deckConfigReviewsTooLow({
                  cards: newValue,
                  expected: Math.min(9999, newValue * 10),
              })
            : "";

    function newEveryDayTab(): ValueTab {
        return new ValueTab(
            tr.deckConfigEveryDay(),
            $config.newPerDay,
            (value) => ($config.newPerDay = value!),
            $config.newPerDay,
            null,
        );
    }

    function reviewEveryDayTab(): ValueTab {
        return new ValueTab(
            tr.deckConfigEveryDay(),
            $config.reviewsPerDay,
            (value) => ($config.reviewsPerDay = value!),
            $config.reviewsPerDay,
            null,
        );
    }

    const newTabs: ValueTab[] = [
        newEveryDayTab(),
        new ValueTab(
            tr.deckConfigTodayOnly(),
            $limits.newTodayActive ? ($limits.newToday ?? null) : null,
            (value) => ($limits.newToday = value ?? undefined),
            null,
            $limits.newToday ?? null,
        ),
    ];

    const reviewTabs: ValueTab[] = [
        reviewEveryDayTab(),
        new ValueTab(
            tr.deckConfigTodayOnly(),
            $limits.reviewTodayActive ? ($limits.reviewToday ?? null) : null,
            (value) => ($limits.reviewToday = value ?? undefined),
            null,
            $limits.reviewToday ?? null,
        ),
    ];

    // the simulator can change these from outside the tabs
    $: if (newTabs[0].value !== $config.newPerDay) {
        newTabs[0] = newEveryDayTab();
    }
    $: if (reviewTabs[0].value !== $config.reviewsPerDay) {
        reviewTabs[0] = reviewEveryDayTab();
    }

    let newValue = 0;
    let reviewsValue = 0;

    const settings = {
        newLimit: {
            title: tr.schedulingNewCardsday(),
            help: tr.deckConfigNewLimitTooltip() + v3Extra,
            url: HelpPage.DeckOptions.newCardsday,
        },
        reviewLimit: {
            title: tr.schedulingMaximumReviewsday(),
            help: tr.deckConfigReviewLimitTooltip() + reviewV3Extra,
            url: HelpPage.DeckOptions.maximumReviewsday,
        },
        newCardsIgnoreReviewLimit: {
            title: tr.deckConfigNewCardsIgnoreReviewLimit(),
            help: newCardsIgnoreReviewLimitHelp,
            url: HelpPage.DeckOptions.newCardsday,
            global: true,
        },
        applyAllParentLimits: {
            title: tr.deckConfigApplyAllParentLimits(),
            help: applyAllParentLimitsHelp,
            url: HelpPage.DeckOptions.limitsFromTop,
            global: true,
        },
    };

    let helpModal: HelpModal;
</script>

<TitledContainer title={tr.deckConfigDailyLimits()}>
    <HelpModal
        title={tr.deckConfigDailyLimits()}
        url={HelpPage.DeckOptions.dailyLimits}
        slot="tooltip"
        sections={settings}
        bind:this={helpModal}
    />
    <SpinBoxRow bind:value={newValue} defaultValue={defaults.newPerDay}>
        <TabbedValue slot="tabs" tabs={newTabs} bind:value={newValue} />
        <SettingTitle on:click={() => helpModal.open("newLimit")}>
            {settings.newLimit.title}
        </SettingTitle>
    </SpinBoxRow>
    <SpinBoxRow bind:value={reviewsValue} defaultValue={defaults.reviewsPerDay}>
        <TabbedValue slot="tabs" tabs={reviewTabs} bind:value={reviewsValue} />
        <SettingTitle on:click={() => helpModal.open("reviewLimit")}>
            {settings.reviewLimit.title}
        </SettingTitle>
    </SpinBoxRow>
    <Warning warning={reviewsTooLow} />
    <SwitchRow bind:value={$newCardsIgnoreReviewLimit} defaultValue={false}>
        <SettingTitle on:click={() => helpModal.open("newCardsIgnoreReviewLimit")}>
            <GlobalLabel title={settings.newCardsIgnoreReviewLimit.title} />
        </SettingTitle>
    </SwitchRow>
    <SwitchRow bind:value={$applyAllParentLimits} defaultValue={false}>
        <SettingTitle on:click={() => helpModal.open("applyAllParentLimits")}>
            <GlobalLabel title={settings.applyAllParentLimits.title} />
        </SettingTitle>
    </SwitchRow>
</TitledContainer>
