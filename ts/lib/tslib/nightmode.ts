// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

function setNightMode(on: boolean): void {
    const root = document.documentElement;
    root.classList.toggle("night-mode", on);
    if (on) {
        root.dataset.bsTheme = "dark";
    } else {
        delete root.dataset.bsTheme;
    }
}

/** Follow the browser's setting, now and as it changes. Returns true if dark. */
export function checkNightMode(): boolean {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    setNightMode(query.matches);
    query.addEventListener("change", (event) => setNightMode(event.matches));
    return query.matches;
}
