// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// What this app uses of bootstrap, as @types/bootstrap would bring in Popper.

declare module "bootstrap/js/dist/modal" {
    export default class Modal {
        constructor(element: Element, options?: Record<string, unknown>);
        show(): void;
        hide(): void;
        toggle(): void;
        dispose(): void;
    }
}

declare module "bootstrap/js/dist/carousel" {
    export default class Carousel {
        constructor(element: Element, options?: Record<string, unknown>);
        to(index: number): void;
        dispose(): void;
    }
}
