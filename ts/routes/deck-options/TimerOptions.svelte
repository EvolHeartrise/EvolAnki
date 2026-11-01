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
    import SwitchRow from "$lib/components/SwitchRow.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";
    import type { HelpItem } from "$lib/components/types";

    import type { DeckOptionsState } from "./lib";
    import SpinBoxRow from "./SpinBoxRow.svelte";
    import Warning from "./Warning.svelte";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const defaults = state.defaults;

    $: maximumAnswerSecondsAboveRecommended =
        $config.capAnswerTimeToSecs > 600
            ? tr.deckConfigMaximumAnswerSecsAboveRecommended()
            : "";

    const settings = {
        maximumAnswerSecs: {
            title: tr.deckConfigMaximumAnswerSecs(),
            help: tr.deckConfigMaximumAnswerSecsTooltip(),
        },
        showAnswerTimer: {
            title: tr.schedulingShowAnswerTimer(),
            help: tr.deckConfigShowAnswerTimerTooltip(),
        },
        stopTimerOnAnswer: {
            title: tr.deckConfigStopTimerOnAnswer(),
            help: tr.deckConfigStopTimerOnAnswerTooltip(),
        },
    };
    const helpSections: HelpItem[] = Object.values(settings);

    let modal: Modal;
    let carousel: Carousel;

    function openHelpModal(index: number): void {
        modal.show();
        carousel.to(index);
    }
</script>

<TitledContainer title={tr.deckConfigTimerTitle()}>
    <HelpModal
        title={tr.deckConfigTimerTitle()}
        url={HelpPage.DeckOptions.timer}
        slot="tooltip"
        {helpSections}
        on:mount={(e) => {
            modal = e.detail.modal;
            carousel = e.detail.carousel;
        }}
    />
    <SpinBoxRow
        bind:value={$config.capAnswerTimeToSecs}
        defaultValue={defaults.capAnswerTimeToSecs}
        min={1}
        max={7200}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("maximumAnswerSecs"))}
        >
            {settings.maximumAnswerSecs.title}
        </SettingTitle>
    </SpinBoxRow>
    <Warning warning={maximumAnswerSecondsAboveRecommended} />
    <!-- AnkiMobile hides this -->
    <div class="show-timer-switch" style="display: contents;">
        <SwitchRow bind:value={$config.showTimer} defaultValue={defaults.showTimer}>
            <SettingTitle
                on:click={() =>
                    openHelpModal(Object.keys(settings).indexOf("showAnswerTimer"))}
            >
                {settings.showAnswerTimer.title}
            </SettingTitle>
        </SwitchRow>
    </div>
    <div class="show-timer-switch" style="display: contents;">
        <SwitchRow
            bind:value={$config.stopTimerOnAnswer}
            defaultValue={defaults.stopTimerOnAnswer}
        >
            <SettingTitle
                on:click={() =>
                    openHelpModal(Object.keys(settings).indexOf("stopTimerOnAnswer"))}
            >
                {settings.stopTimerOnAnswer.title}
            </SettingTitle>
        </SwitchRow>
    </div>
</TitledContainer>
