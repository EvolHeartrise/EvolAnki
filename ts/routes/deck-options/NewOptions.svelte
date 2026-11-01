<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { DeckConfig_Config_NewCardInsertOrder } from "@generated/anki/deck_config_pb";
    import * as tr from "@generated/ftl";
    import { HelpPage } from "@tslib/help-page";
    import type Carousel from "bootstrap/js/dist/carousel";
    import type Modal from "bootstrap/js/dist/modal";
    import EnumSelectorRow from "$lib/components/EnumSelectorRow.svelte";
    import HelpModal from "$lib/components/HelpModal.svelte";
    import SettingTitle from "$lib/components/SettingTitle.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";
    import type { HelpItem } from "$lib/components/types";

    import { newInsertOrderChoices } from "./choices";
    import type { DeckOptionsState } from "./lib";
    import StepsInputRow from "./StepsInputRow.svelte";
    import Warning from "./Warning.svelte";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const defaults = state.defaults;

    let stepsTooLargeForFsrs: string;
    $: {
        const lastLearnStepInDays = $config.learnSteps.length
            ? $config.learnSteps[$config.learnSteps.length - 1] / 60 / 24
            : 0;
        stepsTooLargeForFsrs =
            lastLearnStepInDays >= 1 ? tr.deckConfigStepsTooLargeForFsrs() : "";
    }

    $: insertionOrderRandom =
        $config.newCardInsertOrder == DeckConfig_Config_NewCardInsertOrder.RANDOM
            ? tr.deckConfigNewInsertionOrderRandomWithV3()
            : "";

    const settings = {
        learningSteps: {
            title: tr.deckConfigLearningSteps(),
            help: tr.deckConfigLearningStepsTooltip(),
            url: HelpPage.DeckOptions.learningSteps,
        },
        insertionOrder: {
            title: tr.deckConfigNewInsertionOrder(),
            help: tr.deckConfigNewInsertionOrderTooltip(),
            url: HelpPage.DeckOptions.insertionOrder,
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

<TitledContainer title={tr.schedulingNewCards()}>
    <HelpModal
        title={tr.schedulingNewCards()}
        url={HelpPage.DeckOptions.newCards}
        slot="tooltip"
        {helpSections}
        on:mount={(e) => {
            modal = e.detail.modal;
            carousel = e.detail.carousel;
        }}
    />
    <StepsInputRow bind:value={$config.learnSteps} defaultValue={defaults.learnSteps}>
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("learningSteps"))}
        >
            {settings.learningSteps.title}
        </SettingTitle>
    </StepsInputRow>
    <Warning warning={stepsTooLargeForFsrs} />
    <EnumSelectorRow
        bind:value={$config.newCardInsertOrder}
        defaultValue={defaults.newCardInsertOrder}
        choices={newInsertOrderChoices()}
        breakpoint={"md"}
    >
        <SettingTitle
            on:click={() =>
                openHelpModal(Object.keys(settings).indexOf("insertionOrder"))}
        >
            {settings.insertionOrder.title}
        </SettingTitle>
    </EnumSelectorRow>
    <Warning warning={insertionOrderRandom} />
</TitledContainer>
