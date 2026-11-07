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
    };

    let helpModal: HelpModal;
</script>

<TitledContainer title={tr.deckConfigTimerTitle()}>
    <HelpModal
        title={tr.deckConfigTimerTitle()}
        url={HelpPage.DeckOptions.timer}
        slot="tooltip"
        sections={settings}
        bind:this={helpModal}
    />
    <SpinBoxRow
        bind:value={$config.capAnswerTimeToSecs}
        defaultValue={defaults.capAnswerTimeToSecs}
        min={1}
        max={7200}
    >
        <SettingTitle on:click={() => helpModal.open("maximumAnswerSecs")}>
            {settings.maximumAnswerSecs.title}
        </SettingTitle>
    </SpinBoxRow>
    <Warning warning={maximumAnswerSecondsAboveRecommended} />
</TitledContainer>
