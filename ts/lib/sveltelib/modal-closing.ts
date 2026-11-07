// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { on } from "@tslib/events";

interface ModalClosingHandler {
    set: (value: boolean) => void;
    remove: () => void;
}

/**
 * Register a keydown handler on the document that can optionally stop propagation to other handlers if Escape is pressed and the associated flag is set.
 * Lets a modal take Escape for itself, before the page's own Escape handlers see it.
 */
function registerModalClosingHandler(callback?: () => void): ModalClosingHandler {
    let modalIsOpen = false;

    function set(value: boolean) {
        modalIsOpen = value;
    }

    const remove = on(document, "keydown", (event) => {
        if (event.key === "Escape" && modalIsOpen) {
            event.stopImmediatePropagation();
            if (callback) {
                callback();
            }
        }
    }, { capture: true });

    return { set, remove };
}

export { registerModalClosingHandler };
