<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import * as tr from "@tslib/tr";
    import { HelpPage } from "@tslib/help-page";
    import type Carousel from "bootstrap/js/dist/carousel";
    import type Modal from "bootstrap/js/dist/modal";
    import HelpModal from "$lib/components/HelpModal.svelte";
    import SettingTitle from "$lib/components/SettingTitle.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";
    import type { HelpItem } from "$lib/components/types";

    import type { DeckOptionsState } from "./lib";
    import StepsInputRow from "./StepsInputRow.svelte";
    import Warning from "./Warning.svelte";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const defaults = state.defaults;

    let stepsTooLargeForFsrs: string;
    $: {
        const lastRelearnStepInDays = $config.relearnSteps.length
            ? $config.relearnSteps[$config.relearnSteps.length - 1] / 60 / 24
            : 0;
        stepsTooLargeForFsrs =
            lastRelearnStepInDays >= 1 ? tr.deckConfigStepsTooLargeForFsrs() : "";
    }

    const settings = {
        relearningSteps: {
            title: tr.deckConfigRelearningSteps(),
            help: tr.deckConfigRelearningStepsTooltip(),
            url: HelpPage.DeckOptions.relearningSteps,
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

<TitledContainer title={tr.schedulingLapses()}>
    <HelpModal
        title={tr.schedulingLapses()}
        url={HelpPage.DeckOptions.lapses}
        slot="tooltip"
        {helpSections}
        on:mount={(e) => {
            modal = e.detail.modal;
            carousel = e.detail.carousel;
        }}
    />
    <StepsInputRow
        bind:value={$config.relearnSteps}
        defaultValue={defaults.relearnSteps}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("relearningSteps"))}
        >
            {settings.relearningSteps.title}
        </SettingTitle>
    </StepsInputRow>
    <Warning warning={stepsTooLargeForFsrs} />
</TitledContainer>
