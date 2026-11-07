<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import * as tr from "@tslib/tr";
    import { cloneDeep, isEqual as isEqualLodash } from "lodash-es";

    import { revertIcon } from "$lib/components/icons";

    import Badge from "./Badge.svelte";
    import Icon from "./Icon.svelte";

    type T = unknown;

    export let value: T;
    export let defaultValue: T;

    function isEqual(a: T, b: T): boolean {
        if (typeof a === "number" && typeof b === "number") {
            // round to .01 precision before comparing,
            // so the values coming out of the UI match
            // the originals
            a = Math.round(a * 100) / 100;
            b = Math.round(b * 100) / 100;
        }

        return isEqualLodash(a, b);
    }

    let modified: boolean;
    $: modified = !isEqual(value, defaultValue);

    function revert(): void {
        if (modified) {
            value = cloneDeep(defaultValue);
        }
    }
</script>

<div class:hide={!modified}>
    <Badge
        iconSize={85}
        class="p-1"
        tooltip={tr.deckConfigRevertButtonTooltip()}
        on:click={revert}
    >
        <Icon icon={revertIcon} />
    </Badge>
</div>

<style lang="scss">
    :global(.badge) {
        cursor: pointer;
    }

    .hide :global(.badge) {
        display: none;
        cursor: initial;
    }
</style>
