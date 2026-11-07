<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import Modal from "bootstrap/js/dist/modal";
    import { onDestroy, onMount } from "svelte";

    import { registerModalClosingHandler } from "$lib/sveltelib/modal-closing";

    export let dialogClass: string = "";

    let modal: Modal;
    let modalRef: HTMLDivElement;

    export function show(): void {
        modal.show();
    }

    export function hide(): void {
        modal.hide();
    }

    const { set: setModalOpen, remove: removeModalClosingHandler } =
        registerModalClosingHandler(hide);

    function onShown() {
        setModalOpen(true);
    }

    function onHidden() {
        setModalOpen(false);
    }

    onMount(() => {
        modalRef.addEventListener("shown.bs.modal", onShown);
        modalRef.addEventListener("hidden.bs.modal", onHidden);
        modal = new Modal(modalRef, { keyboard: false });
    });

    onDestroy(() => {
        removeModalClosingHandler();
        modalRef.removeEventListener("shown.bs.modal", onShown);
        modalRef.removeEventListener("hidden.bs.modal", onHidden);
    });
</script>

<div
    bind:this={modalRef}
    class="modal fade"
    tabindex="-1"
    aria-labelledby="modalLabel"
    aria-hidden="true"
>
    <div class="modal-dialog {dialogClass}">
        <div class="modal-content">
            <slot name="header" />
            <slot name="body" />
            <slot name="footer" />
        </div>
    </div>
</div>

<style lang="scss">
    .modal {
        z-index: 1066;
        background-color: rgba($color: black, $alpha: 0.5);
    }

    .modal-content {
        background-color: var(--canvas);
        color: var(--fg);
    }
</style>
