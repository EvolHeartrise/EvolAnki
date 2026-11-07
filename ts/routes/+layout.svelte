<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import "./base.scss";

    import * as tr from "@tslib/tr";
    import { setContext } from "svelte";

    import { modalsKey, touchDeviceKey } from "$lib/components/context-keys";

    setContext(modalsKey, new Map());
    setContext(touchDeviceKey, "ontouchstart" in document.documentElement);
</script>

<!-- Full loads, as pages have different content security policies -->
<nav class="app-nav" data-sveltekit-reload>
    <a href="/">{tr.actionsDecks()}</a>
    <a href="/graphs">Stats</a>
</nav>

<slot />

<style lang="scss">
    .app-nav {
        display: flex;
        justify-content: center;
        gap: 1.5em;
        box-sizing: border-box;
        height: var(--app-nav-height);
        padding-top: env(safe-area-inset-top);
        align-items: center;
        border-bottom: 1px solid var(--border-subtle);
        background: var(--canvas-elevated);

        a {
            color: var(--fg);
            text-decoration: none;
            display: inline-flex;
            align-items: center;
            min-height: 44px;
            min-width: 44px;
            justify-content: center;
            padding: 0 0.75em;
            border-radius: var(--border-radius);

            &:hover {
                background: var(--canvas-inset);
            }
        }
    }
</style>
