<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { slide } from "svelte/transition";

    import Row from "$lib/components/Row.svelte";

    export let warning: string;
    export let className = "alert-warning";
</script>

{#if warning}
    <Row>
        <div class="col-12 alert {className} mb-0" in:slide out:slide>
            {warning}
        </div>
    </Row>
{/if}
