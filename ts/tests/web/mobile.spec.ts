// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Phones, as far as a desktop browser can pretend to be one: the layout, touch, and
// the autoplay policy, in Chromium and WebKit. docs/web-server.md says what this
// can't tell us about real ones.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { type Browser, chromium, devices, expect, type FrameLocator, type Page, test, webkit } from "@playwright/test";

import { Server, tempCollection } from "./server";

/** 0.6 seconds of blue, from ffmpeg -f lavfi -i color=c=blue:s=64x48:r=5 -c:v libvpx. */
const WEBM =
    "GkXfo59ChoEBQveBAULygQRC84EIQoKEd2VibUKHgQJChYECGFOAZwEAAAAAAAIwEU2bdLpNu4tTq4QVSalmU6yBoU27i1OrhBZUrmtTrIHYTbuMU6uEElTDZ1OsggEeTbuMU6uEHFO7a1OsggIa7AEAAAAAAABZAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAVSalmsirXsYMPQkBNgI1MYXZmNjAuMTYuMTAwV0GNTGF2ZjYwLjE2LjEwMESJiECCwAAAAAAAFlSua8GuAQAAAAAAADjXgQFzxYi++y1lhYPGzJyBACK1nIN1bmSIgQCGhVZfVlA4g4EBI+ODhAvrwgDgibCBQLqBMJqBAhJUw2f8c3OgY8CAZ8iaRaOHRU5DT0RFUkSHjUxhdmY2MC4xNi4xMDBzc9ZjwItjxYi++y1lhYPGzGfIoUWjh0VOQ09ERVJEh5RMYXZjNjAuMzEuMTAyIGxpYnZweGfIoUWjiERVUkFUSU9ORIeTMDA6MDA6MDAuNjAwMDAwMDAwAB9DtnX254EAo8GBAACAUAMAnQEqQAAwAABHCIWFiIWEiAICAnWqA/gCBwjVZ5mPN9IA/v9NEv/8WFfxYV/FhX/xYV/8/M7txfzmAKOWgQDIANEBAAEQEAAYABhYL/QACIwAAKOWgQGQANEBAAEQEAAYABhYL/QACIwAABxTu2uRu4+zgQC3iveBAfGCAZ/wgQM=";

function wav(): Buffer {
    const samples = 4410;
    const header = Buffer.alloc(44);
    header.write("RIFF", 0);
    header.writeUInt32LE(36 + samples, 4);
    header.write("WAVEfmt ", 8);
    header.writeUInt32LE(16, 16);
    header.writeUInt16LE(1, 20);
    header.writeUInt16LE(1, 22);
    header.writeUInt32LE(44100, 24);
    header.writeUInt32LE(44100, 28);
    header.writeUInt16LE(1, 32);
    header.writeUInt16LE(8, 34);
    header.write("data", 36);
    header.writeUInt32LE(samples, 40);
    return Buffer.concat([header, Buffer.alloc(samples, 128)]);
}

let collection: ReturnType<typeof tempCollection>;
let server: Server;
let browser: Browser;

async function start(notes: { deck: string; notetype: string; fields: string[] }[], decks: string[]): Promise<void> {
    const media = collection.path.replace(/\.anki2$/, ".media");
    mkdirSync(media, { recursive: true });
    writeFileSync(join(media, "beep.wav"), wav());
    writeFileSync(join(media, "clip.webm"), Buffer.from(WEBM, "base64"));
    const seed = join(dirname(collection.path), "seed.json");
    writeFileSync(seed, JSON.stringify({ decks: decks.map((name) => ({ name })), notes }));
    server = await Server.start(collection.path, seed);
}

test.beforeEach(() => {
    collection = tempCollection();
});

test.afterEach(async () => {
    await browser?.close();
    server?.kill();
    collection.cleanup();
});

const basic = (deck: string, front: string, back: string) => ({ deck, notetype: "Basic", fields: [front, back] });

function card(page: Page): FrameLocator {
    return page.frameLocator("iframe.sandboxed-frame");
}

async function openDeck(page: Page, name: string): Promise<void> {
    await page.goto(server.url);
    await page.locator(`tr[data-deck="${name}"] .name a`).tap();
    await expect(page.locator("main.overview > h1")).toContainText(name);
}

async function noSidewaysScroll(page: Page, what: string): Promise<void> {
    const [scroll, inner] = await page.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]);
    expect(scroll, what).toBeLessThanOrEqual(inner);
}

for (
    const { label, launch, device } of [
        // with the policy that a real phone has, as a desktop's is more relaxed
        {
            label: "Android Chrome",
            launch: () => chromium.launch({ args: ["--autoplay-policy=document-user-activation-required"] }),
            device: devices["Pixel 7"],
        },
        { label: "iPhone Safari", launch: () => webkit.launch(), device: devices["iPhone 14"] },
    ]
) {
    test.describe(label, () => {
        test("pages fit the screen, with big targets and thumb-height answer buttons", async () => {
            browser = await launch();
            const context = await browser.newContext({ ...device, colorScheme: "dark" });
            const page = await context.newPage();
            await start(
                [
                    basic("Languages::French with quite a long name for a deck", "bonjour", "hello"),
                    basic("Science", "a question with a rather-long-unbroken-word-in-it-to-wrap", "answer"),
                ],
                ["Languages", "Languages::French with quite a long name for a deck", "Science"],
            );
            await page.goto(server.url);
            await page.locator(`tr[data-deck="Languages"]`).getByRole("button").tap();
            await page.locator(`tr[data-deck="French with quite a long name for a deck"]`).waitFor();
            await noSidewaysScroll(page, "deck list");
            for (const target of await page.locator("main.decks a, main.decks button, .app-nav a").all()) {
                const box = (await target.boundingBox())!;
                expect(box.height, await target.innerText()).toBeGreaterThanOrEqual(44);
            }

            await page.locator(`tr[data-deck="Science"] .name a`).tap();
            await page.getByRole("button", { name: "Study Now" }).waitFor();
            await noSidewaysScroll(page, "overview");
            await page.getByRole("button", { name: "Study Now" }).tap();

            const frame = page.locator("iframe.sandboxed-frame");
            await expect(card(page).locator("#qa")).toContainText("question");
            await noSidewaysScroll(page, "reviewer");
            const viewport = page.viewportSize()!;
            const showAnswer = page.getByRole("button", { name: "Show Answer" });
            const showBox = (await showAnswer.boundingBox())!;
            expect(showBox.height).toBeGreaterThanOrEqual(48);
            expect(showBox.width).toBeGreaterThan(viewport.width * 0.8);
            expect(showBox.y + showBox.height).toBeGreaterThan(viewport.height - 60);

            // the frame fills what's left, and its content doesn't overflow it
            const frameBox = (await frame.boundingBox())!;
            expect(frameBox.width).toBe(viewport.width);
            expect(frameBox.y + frameBox.height).toBeLessThanOrEqual(showBox.y);
            expect(frameBox.height).toBeGreaterThan(viewport.height * 0.5);
            const inner = await card(page).locator("html").evaluate((html) => [html.scrollWidth, innerWidth]);
            expect(inner[0]).toBeLessThanOrEqual(inner[1]);

            await showAnswer.tap();
            const rating = page.locator("button[data-rating='3']");
            await expect(rating).toBeVisible();
            await noSidewaysScroll(page, "reviewer answer");
            const boxes = await Promise.all(
                [1, 2, 3, 4].map((n) => page.locator(`button[data-rating='${n}']`).boundingBox()),
            );
            for (const box of boxes) {
                expect(box!.height).toBeGreaterThanOrEqual(48);
                expect(box!.width).toBeGreaterThanOrEqual(64);
                expect(box!.y + box!.height).toBeGreaterThan(viewport.height - 60);
            }
            await rating.tap();
            await expect(page).toHaveURL(/\/review$|\/overview\/\d+$/);

            await page.goto(`${server.url}/deck-options/1`);
            await page.locator(".setting-title", { hasText: "New cards/day" }).first().waitFor();
            await noSidewaysScroll(page, "deck options");

            await page.goto(`${server.url}/graphs`);
            await page.locator("svg").first().waitFor();
            await noSidewaysScroll(page, "graphs");
        });

        test("sound plays after the tap on Study Now, with no further asking", async () => {
            browser = await launch();
            const context = await browser.newContext({ ...device });
            const page = await context.newPage();
            await start([basic("Sound", "[sound:beep.wav]question", "[sound:beep.wav]answer")], ["Sound"]);
            await openDeck(page, "Sound");
            await page.getByRole("button", { name: "Study Now" }).tap();
            await expect(card(page).locator("#qa")).toContainText("question");
            const audio = page.locator("audio");
            await expect.poll(() => audio.evaluate((a: HTMLAudioElement) => a.ended && a.src.endsWith("beep.wav")))
                .toBe(true);
            await expect(page.getByRole("button", { name: /Tap to play/ })).toHaveCount(0);

            // and again after the await-laden reveal, which is no longer inside the tap
            await audio.evaluate((a: HTMLAudioElement) => {
                (window as any).playedAgain = false;
                a.addEventListener("playing", () => ((window as any).playedAgain = true));
            });
            await page.getByRole("button", { name: "Show Answer" }).tap();
            await expect.poll(() => page.evaluate(() => (window as any).playedAgain)).toBe(true);
            await expect(page.getByRole("button", { name: /Tap to play/ })).toHaveCount(0);
        });

        test("when the browser refuses, a clear control plays the sound", async () => {
            browser = await launch();
            const context = await browser.newContext({ ...device });
            const page = await context.newPage();
            await start([basic("Blocked", "[sound:beep.wav]question", "x")], ["Blocked"]);
            // WebKit on Linux has no autoplay policy to speak of, so act out one that
            // has no memory of past gestures, while the real one applies in Chromium.
            await page.addInitScript(() => {
                const realPlay = HTMLMediaElement.prototype.play;
                let tapped = false;
                addEventListener("click", () => (tapped = true), true);
                HTMLMediaElement.prototype.play = function() {
                    return tapped
                        ? realPlay.call(this)
                        : Promise.reject(new DOMException("refused", "NotAllowedError"));
                };
            });
            // opened directly, as after a reload: no gesture has happened on this page
            await openDeck(page, "Blocked");
            await page.goto(`${server.url}/review`);
            await expect(card(page).locator("#qa")).toContainText("question");
            const control = page.getByRole("button", { name: /Tap to play/ });
            await expect(control).toBeVisible();
            const box = (await control.boundingBox())!;
            expect(box.height).toBeGreaterThanOrEqual(44);
            await noSidewaysScroll(page, "with the play control");

            await control.tap();
            const audio = page.locator("audio");
            await expect.poll(() => audio.evaluate((a: HTMLAudioElement) => a.src)).toMatch(/beep\.wav$/);
            await expect.poll(() => audio.evaluate((a: HTMLAudioElement) => a.ended)).toBe(true);
            await expect(control).toHaveCount(0);
        });

        test("TTS tags are spoken, and sound/video files find the right element", async () => {
            browser = await launch();
            const context = await browser.newContext({ ...device });
            const page = await context.newPage();
            // Not all of these browsers have a voice here, so stand in for the engine.
            await page.addInitScript(() => {
                (window as any).spoken = [];
                (window as any).SpeechSynthesisUtterance = class {
                    lang = "";
                    rate = 1;
                    voice = null;
                    volume = 1;
                    onend: (() => void) | null = null;
                    onerror = null;
                    constructor(readonly text: string) {}
                };
                Object.defineProperty(window, "speechSynthesis", {
                    value: {
                        getVoices: () => [],
                        cancel: () => undefined,
                        speak(utterance: any) {
                            if (utterance.text) {
                                (window as any).spoken.push({
                                    text: utterance.text,
                                    lang: utterance.lang,
                                    rate: utterance.rate,
                                });
                            }
                            setTimeout(() => utterance.onend?.(), 0);
                        },
                    },
                });
            });
            await start(
                [
                    {
                        deck: "Speech",
                        notetype: "Basic",
                        fields: ["[anki:tts lang=ja_JP speed=1.5]こんにちは[/anki:tts]", "[sound:clip.webm]answer"],
                    },
                ],
                ["Speech"],
            );
            await openDeck(page, "Speech");
            await page.getByRole("button", { name: "Study Now" }).tap();
            await expect(card(page).locator("#qa")).toBeAttached();
            await expect.poll(() => page.evaluate(() => (window as any).spoken))
                .toEqual([{ text: "こんにちは", lang: "ja-JP", rate: 1.5 }]);
            await expect(page.locator("video")).toBeHidden();

            await page.getByRole("button", { name: "Show Answer" }).tap();
            const video = page.locator("video");
            await expect(video).toBeVisible();
            await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.src)).toMatch(/clip\.webm$/);
            await expect.poll(() => page.locator("audio").evaluate((a: HTMLAudioElement) => a.src)).not.toMatch(
                /clip\.webm$/,
            );
            const box = (await video.boundingBox())!;
            expect(box.width).toBeLessThanOrEqual(page.viewportSize()!.width);
            await noSidewaysScroll(page, "with a video");
            // moving on takes the video away
            await page.locator("button[data-rating='3']").tap();
            await expect(video).toBeHidden();
        });
    });
}

test("a video that the browser can play reaches its end", async () => {
    browser = await chromium.launch();
    const page = await browser.newPage();
    await start([basic("Clip", "[sound:clip.webm]q", "a")], ["Clip"]);
    await page.goto(server.url);
    await page.locator(`tr[data-deck="Clip"] .name a`).click();
    await page.getByRole("button", { name: "Study Now" }).click();
    const video = page.locator("video");
    await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.ended)).toBe(true);
    await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.videoWidth)).toBe(64);
});
