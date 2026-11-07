<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import * as tr from "@tslib/tr";
    import { HelpPage } from "@tslib/help-page";
    import HelpModal from "$lib/components/HelpModal.svelte";
    import SettingTitle from "$lib/components/SettingTitle.svelte";
    import TitledContainer from "$lib/components/TitledContainer.svelte";

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

    let helpModal: HelpModal;
</script>

<TitledContainer title={tr.schedulingLapses()}>
    <HelpModal
        title={tr.schedulingLapses()}
        url={HelpPage.DeckOptions.lapses}
        slot="tooltip"
        sections={settings}
        bind:this={helpModal}
    />
    <StepsInputRow
        bind:value={$config.relearnSteps}
        defaultValue={defaults.relearnSteps}
    >
        <SettingTitle on:click={() => helpModal.open("relearningSteps")}>
            {settings.relearningSteps.title}
        </SettingTitle>
    </StepsInputRow>
    <Warning warning={stepsTooLargeForFsrs} />
</TitledContainer>
