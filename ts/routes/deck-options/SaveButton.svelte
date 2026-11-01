<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { UpdateDeckConfigsMode } from "@generated/anki/deck_config_pb";
    import * as tr from "@generated/ftl";
    import { getPlatformString } from "@tslib/shortcuts";

    import LabelButton from "$lib/components/LabelButton.svelte";
    import Shortcut from "$lib/components/Shortcut.svelte";

    import type { DeckOptionsState } from "./lib";
    import { commitEditing, saveWithProgress } from "./lib";

    export let state: DeckOptionsState;

    async function save(mode: UpdateDeckConfigsMode): Promise<void> {
        await commitEditing();
        await saveWithProgress(state, mode);
        window.location.href = `/overview/${state.targetDeckId}?saved=1`;
    }

    const saveKeyCombination = "Control+Enter";
</script>

<LabelButton
    primary
    on:click={() => save(UpdateDeckConfigsMode.NORMAL)}
    tooltip={getPlatformString(saveKeyCombination)}
>
    <div class="save">{tr.deckConfigSaveButton()}</div>
</LabelButton>
<Shortcut
    keyCombination={saveKeyCombination}
    on:action={() => save(UpdateDeckConfigsMode.NORMAL)}
/>

<style lang="scss">
    .save {
        margin: 0 0.75rem;
    }
</style>
