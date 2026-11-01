<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

The only way to show untrusted HTML in the web app. See docs/web-server.md.
-->
<script lang="ts">
    import { type FrameMessage, parseFrameMessage } from "@tslib/frame-protocol";
    import { onDestroy } from "svelte";

    /** From getFrameBase(). */
    export let base: string;
    /** "card" also loads the reviewer's own scripts. Neither runs any in the content. */
    export let mode: "card" | "static" = "card";
    export let onMessage: (message: FrameMessage) => void;
    /** The frame went somewhere else, and has been replaced with a blank one. */
    export let onNavigated: () => void = () => {};

    /** Replacing the element is the only way to start over. */
    let generation = 0;
    let port: MessagePort | null = null;
    let handedOver = false;
    let frame: HTMLIFrameElement;

    $: src = `${base}m/${mode === "static" ? "?static" : ""}`;

    function onLoad(): void {
        // The first load is our own document, with nothing untrusted in it yet.
        // Another means that content navigated the frame. Whatever's there now
        // must not get a port, as that would let it press keys for the user.
        if (handedOver) {
            closePort();
            handedOver = false;
            generation += 1;
            onNavigated();
            return;
        }
        handedOver = true;
        const channel = new MessageChannel();
        port = channel.port1;
        port.onmessage = (event) => {
            const message = parseFrameMessage(event.data);
            if (message) {
                onMessage(message);
            }
        };
        // a sandboxed document's origin can't be named
        frame.contentWindow!.postMessage({ type: "init" }, "*", [channel.port2]);
    }

    function closePort(): void {
        port?.close();
        port = null;
    }

    export function send(message: Record<string, unknown>): void {
        port?.postMessage(message);
    }

    onDestroy(closePort);
</script>

{#key generation}
    <!-- The sandbox has no allow-same-origin, or the content could reach the page. -->
    <iframe
        bind:this={frame}
        class="sandboxed-frame"
        title="content"
        {src}
        sandbox="allow-scripts"
        on:load={onLoad}
    ></iframe>
{/key}

<style>
    .sandboxed-frame {
        display: block;
        width: 100%;
        height: 100%;
        border: 0;
    }
</style>
