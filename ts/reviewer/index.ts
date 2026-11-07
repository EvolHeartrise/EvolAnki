// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Shows cards in the sandboxed frame; see rslib/web-server/src/card_frame.html.
// Scripts in a card never run: innerHTML doesn't start them, and the frame's
// CSP refuses them and inline handlers.

/* eslint
@typescript-eslint/no-explicit-any: "off",
 */

import { addBrowserClasses } from "./browser_selector";
import { allImagesLoaded, preloadAnswerImages } from "./images";
import { preloadResources } from "./preload";

declare const MathJax: any;

let mathjaxLoading: Promise<void> | null = window?.["MathJax"]?.startup?.promise ?? null;

function _lazyLoadMathJax(): Promise<void> {
    // Set by the web app's sandboxed card frame, which can't load from /_anki/.
    const assets: string = globalThis.ankiAssetBase ?? "/_anki/";
    return mathjaxLoading || (mathjaxLoading = new Promise((resolve, reject) => {
        const configScript = document.createElement("script");
        configScript.src = `${assets}js/mathjax.js`;
        configScript.onload = () => {
            const mathjaxScript = document.createElement("script");
            mathjaxScript.src = `${assets}js/vendor/mathjax/tex-chtml-full.js`;
            mathjaxScript.onload = () => resolve();
            mathjaxScript.onerror = () => reject(new Error("Failed to load MathJax"));
            document.head.appendChild(mathjaxScript);
        };
        configScript.onerror = () => reject(new Error("Failed to load MathJax config"));
        document.head.appendChild(configScript);
    }));
}

// follows mathjaxBlockDelimiterPattern and mathjaxInlineDelimiterPattern
const mathjaxRegex = /\\\[(.*?)\\\]|\\\((.*?)\\\)/su;

function _containsMathjax(html: string): boolean {
    return mathjaxRegex.test(html);
}

type Callback = () => void | Promise<void>;

async function runCallback(callback: Callback): Promise<void> {
    try {
        await callback();
    } catch (error) {
        console.log("Callback failed: ", error);
    }
}

let _updatingQueue: Promise<void> = Promise.resolve();

export function _queueAction(action: Callback): void {
    _updatingQueue = _updatingQueue.then(action);
}

const renderError = (type: string) => (error: unknown): string => {
    const errorMessage = String(error).substring(0, 2000);
    let errorStack: string;
    if (error instanceof Error) {
        errorStack = String(error.stack).substring(0, 2000);
    } else {
        errorStack = "";
    }
    return `<div>Invalid ${type} on card: ${errorMessage}\n${errorStack}</div>`.replace(
        /\n/g,
        "<br>",
    );
};

export async function _updateQA(
    html: string,
    onupdate: Callback,
    onshown: Callback,
): Promise<void> {
    const qa = document.getElementById("qa")!;

    const containsMathJax = _containsMathjax(html);
    if (containsMathJax) {
        try {
            await _lazyLoadMathJax();
        } catch (error) {
            console.error(error);
        }
    }
    await preloadResources(html);

    qa.style.opacity = "0";

    try {
        qa.innerHTML = html;
    } catch (error) {
        qa.innerHTML = renderError("html")(error);
    }

    await runCallback(onupdate);

    if (containsMathJax && typeof MathJax !== "undefined" && MathJax.startup) {
        // wait for mathjax to ready
        await MathJax.startup.promise
            .then(() => {
                // clear MathJax buffers from previous typesets
                MathJax.typesetClear();

                return MathJax.typesetPromise([qa]);
            })
            .catch(renderError("MathJax"));
    }

    qa.style.opacity = "1";

    await runCallback(onshown);
}

export function _showQuestion(q: string, a: string, bodyclass: string): void {
    _queueAction(() =>
        _updateQA(
            q,
            function() {
                // return to top of window
                window.scrollTo(0, 0);

                document.body.className = bodyclass;
            },
            function() {
                // focus typing area if visible
                document.getElementById("typeans")?.focus();
                if (!mathjaxLoading && _containsMathjax(a)) {
                    _lazyLoadMathJax();
                }
                // preload images
                allImagesLoaded().then(() => preloadAnswerImages(a));
            },
        )
    );
}

function scrollToAnswer(): void {
    document.getElementById("answer")?.scrollIntoView();
}

export function _showAnswer(a: string, bodyclass: string): void {
    _queueAction(() =>
        _updateQA(
            a,
            function() {
                if (bodyclass) {
                    //  when previewing
                    document.body.className = bodyclass;
                }

                // avoid scrolling to the answer until images load
                allImagesLoaded().then(scrollToAnswer);
            },
            function() {
                /* noop */
            },
        )
    );
}

addBrowserClasses();
