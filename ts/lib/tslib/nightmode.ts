// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { isWebApp } from "./webapp";

function setNightMode(on: boolean): void {
    const root = document.documentElement;
    root.classList.toggle("night-mode", on);
    if (on) {
        root.dataset.bsTheme = "dark";
    } else {
        delete root.dataset.bsTheme;
    }
}

/** Add night-mode class to documentElement if hash location is #night, or in
    the web app, if the browser asks for a dark theme. Returns true if added.
    In the web app, the class then follows any change in the browser's setting. */
export function checkNightMode(): boolean {
    if (isWebApp()) {
        const query = window.matchMedia("(prefers-color-scheme: dark)");
        setNightMode(query.matches);
        query.addEventListener("change", (event) => setNightMode(event.matches));
        return query.matches;
    }
    const nightMode = window.location.hash == "#night";
    if (nightMode) {
        document.documentElement.className = "night-mode";
        document.documentElement.dataset.bsTheme = "dark";
    }
    return nightMode;
}
