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

    import type { DeckOptionsState } from "./lib";

    export let state: DeckOptionsState;

    const config = state.currentConfig;
    const defaults = state.defaults;

    const settings = {
        disableAutoplay: {
            title: tr.deckConfigDisableAutoplay(),
            help: tr.deckConfigDisableAutoplayTooltip(),
        },
        skipQuestionWhenReplaying: {
            title: tr.deckConfigSkipQuestionWhenReplaying(),
            help: tr.deckConfigAlwaysIncludeQuestionAudioTooltip(),
        },
    };

    let helpModal: HelpModal;
</script>

<TitledContainer title={tr.deckConfigAudioTitle()}>
    <HelpModal
        title={tr.deckConfigAudioTitle()}
        url={HelpPage.DeckOptions.audio}
        slot="tooltip"
        sections={settings}
        bind:this={helpModal}
    />
    <SwitchRow
        bind:value={$config.disableAutoplay}
        defaultValue={defaults.disableAutoplay}
    >
        <SettingTitle on:click={() => helpModal.open("disableAutoplay")}>
            {settings.disableAutoplay.title}
        </SettingTitle>
    </SwitchRow>
    <SwitchRow
        bind:value={$config.skipQuestionWhenReplayingAnswer}
        defaultValue={defaults.skipQuestionWhenReplayingAnswer}
    >
        <SettingTitle on:click={() => helpModal.open("skipQuestionWhenReplaying")}>
            {settings.skipQuestionWhenReplaying.title}
        </SettingTitle>
    </SwitchRow>
</TitledContainer>
