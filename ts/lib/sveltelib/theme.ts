// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { get, readable } from "svelte/store";

interface ThemeInfo {
    isDark: boolean;
}

function getThemeFromRoot(): ThemeInfo {
    return {
        isDark: document.documentElement.classList.contains("night-mode"),
    };
}

let setPageTheme: ((theme: ThemeInfo) => void) | null = null;
/** The current theme that applies to this document. */
export const pageTheme = readable(getThemeFromRoot(), (set) => {
    setPageTheme = set;
});
// ensure setPageTheme is set immediately
get(pageTheme);

// Update theme when root element's class changes.
const observer = new MutationObserver((_mutationsList, _observer) => {
    setPageTheme!(getThemeFromRoot());
});
observer.observe(document.documentElement, { attributeFilter: ["class"] });
