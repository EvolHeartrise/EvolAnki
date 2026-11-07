<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script context="module" lang="ts">
    export interface Choice<T> {
        label: string;
        value: T;
    }
</script>

<script lang="ts">
    import { chevronDown } from "$lib/components/icons";

    import Icon from "./Icon.svelte";
    import IconConstrain from "./IconConstrain.svelte";

    type T = $$Generic;

    export let value: T;
    export let choices: Choice<T>[] = [];
    export let disabled: boolean = false;
    export let disabledChoices: T[] = [];
</script>

<div class="enum-selector" class:disabled>
    <select bind:value {disabled}>
        {#each choices as choice}
            <option
                value={choice.value}
                disabled={disabledChoices.includes(choice.value)}
            >
                {choice.label}
            </option>
        {/each}
    </select>
    <div class="chevron">
        <IconConstrain iconSize={80}>
            <Icon icon={chevronDown} />
        </IconConstrain>
    </div>
</div>

<style lang="scss">
    @use "../sass/button-mixins" as button;

    .enum-selector {
        position: relative;
        height: 100%;
    }

    select {
        @include button.select($with-disabled: false);
        appearance: none;
        height: 100%;
        line-height: 1.5;
        padding-block: 0;
        padding-inline: 0.5rem calc(var(--buttons-size, 22px) + 0.5rem);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    option {
        background: var(--canvas-elevated);
        color: var(--fg);
    }

    .chevron {
        position: absolute;
        top: 1px;
        bottom: 1px;
        inset-inline-end: 1px;
        display: flex;
        align-items: center;
        pointer-events: none;
        border-inline-start: 1px solid var(--border-subtle);
    }

    .disabled {
        opacity: 0.5;

        select {
            pointer-events: none;
        }
    }
</style>
