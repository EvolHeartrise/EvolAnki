<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

User-written HTML in a sandboxed frame without any scripts, sized to its content.
-->
<script lang="ts">
    import type { FrameMessage } from "@tslib/frame-protocol";
    import { getFrameBase } from "@tslib/webapp";

    import { pageTheme } from "$lib/sveltelib/theme";

    import SandboxedFrame from "./SandboxedFrame.svelte";

    export let html: string;

    let frame: SandboxedFrame;
    let ready = false;
    let height = 0;
    let navigations = 0;

    $: if (ready) {
        frame.send({ type: "static", html, dark: $pageTheme.isDark });
    }

    function onMessage(message: FrameMessage): void {
        if (message.type === "ready") {
            ready = true;
        } else if (message.type === "resize") {
            height = message.height;
        } else if (message.type === "open") {
            window.open(message.url, "_blank", "noopener,noreferrer");
        }
    }

    function onNavigated(): void {
        ready = false;
        navigations += 1;
        // it can't escape the frame, but mustn't keep the page busy either
        if (navigations >= 3) {
            html = "";
        }
    }
</script>

{#await getFrameBase() then base}
    <div class="untrusted-html" style:height="{height}px">
        <SandboxedFrame
            bind:this={frame}
            {base}
            mode="static"
            {onMessage}
            onNavigated={onNavigated}
        />
    </div>
{/await}

<style>
    .untrusted-html {
        width: 100%;
        max-height: 60vh;
        overflow: hidden;
    }
</style>
