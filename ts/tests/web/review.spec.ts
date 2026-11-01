// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// The study flow on seeded collections, and what untrusted content can do to it.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { type Browser, devices, expect, type FrameLocator, type Page, test } from "@playwright/test";

import { Server, tempCollection } from "./server";

/** Where the malicious card records the result of each attempt. */
const ATTACK_LOG = "attack-log";

/** Tries everything a hostile shared deck might like to do, if it could run. */
const ATTACK_SCRIPT = `<div id="${ATTACK_LOG}">pending</div><script>
(async () => {
    const results = {};
    const attempt = async (name, fn) => {
        try { results[name] = String(await fn()); } catch (e) { results[name] = "threw " + e.name; }
    };
    const post = (path, init) => fetch(path, {
        method: "POST", headers: {"Content-Type": "application/binary"}, body: new Uint8Array(), ...init,
    }).then(r => r.status);
    await attempt("parentDocument", () => window.parent.document.title);
    await attempt("topDocument", () => window.top.document.title);
    await attempt("fetchAnswer", () => post("/_anki/answerCard"));
    await attempt("xhr", () => new Promise((resolve) => {
        const x = new XMLHttpRequest();
        x.open("POST", "/_anki/undo");
        x.onload = () => resolve(x.status);
        x.onerror = () => resolve("error");
        x.send();
    }));
    await attempt("cookie", () => document.cookie);
    await attempt("storage", () => localStorage.length);
    // nothing in the parent listens, but it would count as answering if it did
    for (const key of ["1", "2", "3", "4", " ", "u", "@"]) {
        window.parent.postMessage({ type: "key", key, repeat: false }, "*");
        window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    }
    window.top.postMessage({ type: "answerCard" }, "*");
    await attempt("topNavigate", () => { window.top.location.href = "/graphs"; return "no error"; });
    await attempt("popup", () => window.open("/graphs"));
    document.getElementById("${ATTACK_LOG}").textContent = JSON.stringify(results);
    window.top.postMessage({ results }, "*");
})();
</script>`;

interface Seed {
    decks?: { name: string; description?: string; markdown?: boolean }[];
    notes?: { deck?: string; notetype: string; fields: string[]; due?: boolean }[];
    notetypes?: { name: string; fields: string[]; front: string; back: string }[];
}

/** A short silent WAV, which any browser can play. */
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

const PIXEL = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==",
    "base64",
);

let collection: ReturnType<typeof tempCollection>;
let server: Server;

async function start(seed: Seed): Promise<void> {
    const media = collection.path.replace(/\.anki2$/, ".media");
    mkdirSync(media, { recursive: true });
    writeFileSync(join(media, "sound file.wav"), wav());
    writeFileSync(join(media, "pixel.png"), PIXEL);
    const seedPath = join(dirname(collection.path), "seed.json");
    writeFileSync(seedPath, JSON.stringify(seed));
    server = await Server.start(collection.path, seedPath);
}

test.beforeEach(() => {
    collection = tempCollection();
});

test.afterEach(() => {
    server?.kill();
    collection.cleanup();
});

/** Stops the server, so nothing else holds the database, and runs a query. */
async function query<T>(sql: string): Promise<T[]> {
    await server.stop();
    const db = new DatabaseSync(collection.path, { readOnly: true });
    try {
        return db.prepare(sql).all() as T[];
    } finally {
        db.close();
    }
}

function failOnPageProblems(page: Page, allowed: RegExp[] = []): string[] {
    const problems: string[] = [];
    const add = (text: string) => {
        if (!allowed.some((pattern) => pattern.test(text))) {
            problems.push(text);
        }
    };
    page.on("dialog", (dialog) => {
        add(`dialog: ${dialog.message()}`);
        void dialog.dismiss();
    });
    page.on("pageerror", (error) => add(`pageerror: ${error.message}`));
    page.on("console", (message) => {
        if (message.type() === "error") {
            add(`console: ${message.text()}`);
        }
    });
    page.on("response", (response) => {
        if (response.status() >= 400) {
            add(`${response.status()}: ${response.url()}`);
        }
    });
    return problems;
}

function cardFrame(page: Page): FrameLocator {
    return page.frameLocator("iframe.sandboxed-frame");
}

function card(page: Page) {
    return cardFrame(page).locator("#qa");
}

function counts(page: Page) {
    return page.locator(".counts");
}

async function openDeck(page: Page, name: string): Promise<void> {
    await page.goto(server.url);
    await page.locator(`tr[data-deck="${name}"] .name a`).click();
    await expect(page.locator("main.overview > h1")).toContainText(name);
}

async function study(page: Page, name: string): Promise<void> {
    await openDeck(page, name);
    await page.getByRole("button", { name: "Study Now" }).click();
    await expect(page.locator(".reviewer")).toBeVisible();
}

const BASIC = (front: string, back: string, deck?: string) => ({
    deck,
    notetype: "Basic",
    fields: [front, back],
});

test("deck list shows the tree, its counts, and collapses", async ({ page }) => {
    await start({
        decks: [{ name: "Parent" }, { name: "Parent::Child" }, { name: "Other" }],
        notes: [
            BASIC("p1", "a", "Parent"),
            BASIC("c1", "a", "Parent::Child"),
            BASIC("c2", "a", "Parent::Child"),
            { ...BASIC("o1", "a", "Other"), due: true },
        ],
    });
    const problems = failOnPageProblems(page);
    await page.goto(server.url);

    const row = (name: string) => page.locator(`tr[data-deck="${name}"]`);
    // new decks start collapsed, as in Qt
    await expect(row("Parent").locator(".new-count")).toHaveText("3");
    await expect(row("Child")).toHaveCount(0);
    await row("Parent").getByRole("button").click();
    await expect(row("Child").locator(".new-count")).toHaveText("2");
    await page.reload();
    await expect(row("Child")).toBeVisible();
    await expect(row("Other").locator(".new-count")).toHaveText("0");
    await expect(row("Other").locator(".review-count")).toHaveText("1");
    // the empty default deck is left out
    await expect(row("Default")).toHaveCount(0);

    await row("Parent").getByRole("button").click();
    await expect(row("Child")).toHaveCount(0);
    await page.reload();
    await expect(row("Parent")).toBeVisible();
    await expect(row("Child")).toHaveCount(0);
    await row("Parent").getByRole("button").click();
    await expect(row("Child")).toBeVisible();

    await expect(page.getByRole("link", { name: "Stats" })).toHaveAttribute("href", "/graphs");
    await expect(row("Other").getByRole("link", { name: /Options/ }))
        .toHaveAttribute("href", /\/deck-options\/\d+/);

    await row("Other").locator(".name a").click();
    await expect(page).toHaveURL(/\/overview\/\d+$/);
    await expect(page.locator(".review-count")).toHaveText("1");
    await expect(page.locator(".new-count")).toHaveText("0");
    expect(problems).toEqual([]);
});

test("a deck can be studied to the end, and the database reflects it", async ({ page }) => {
    await start({
        decks: [{ name: "Study", description: "<b>About</b> this deck" }],
        notes: [BASIC("one", "first", "Study"), BASIC("two", "second", "Study")],
    });
    const problems = failOnPageProblems(page);
    await openDeck(page, "Study");
    await expect(
        page.frameLocator(".description iframe").locator("#content b"),
    ).toHaveText("About");
    await page.getByRole("button", { name: "Study Now" }).click();

    await expect(card(page)).toContainText("one");
    await expect(counts(page)).toContainText("2");
    await expect(card(page)).not.toContainText("first");

    // measured from when the question was shown
    const THINK_MILLIS = 700;
    await page.waitForTimeout(THINK_MILLIS);
    await page.getByRole("button", { name: "Show Answer" }).click();
    await expect(card(page)).toContainText("first");
    for (const [rating, label] of [[1, "Again"], [2, "Hard"], [3, "Good"], [4, "Easy"]]) {
        const button = page.locator(`button[data-rating="${rating}"]`);
        await expect(button).toContainText(label as string);
        // the interval from DescribeNextStates
        await expect(button.locator(".interval")).toHaveText(/\S/);
    }
    await page.locator("button[data-rating='4']").click();

    await expect(card(page)).toContainText("two");
    await page.getByRole("button", { name: "Show Answer" }).click();
    await page.locator("button[data-rating='4']").click();

    // finishing goes to the deck list, where the deck has nothing left
    await expect(page).toHaveURL(new RegExp(`^${server.url}/$`));
    const row = page.locator("main.decks tbody tr").first();
    await expect(row).toBeVisible();
    await expect(row.locator("td").nth(1)).toHaveText("0");
    await expect(row.locator("td").nth(2)).toHaveText("0");
    await expect(row.locator("td").nth(3)).toHaveText("0");
    expect(problems).toEqual([]);

    // going back doesn't return to the empty reviewer
    await page.goBack();
    await expect(page).toHaveURL(/\/overview\/\d+$/);
    await expect(page.getByRole("button", { name: "Study Now" })).toHaveCount(0);
    await expect(page.locator(".finished")).toBeVisible();
    await expect(page.locator(".new-count")).toHaveText("0");

    // opening the finished deck stays on its overview, and the reviewer sends you on
    await page.getByRole("main").getByRole("link", { name: "Decks" }).click();
    await row.locator(".name a").click();
    await expect(page).toHaveURL(/\/overview\/\d+$/);
    await expect(page.locator(".finished")).toBeVisible();
    await page.waitForTimeout(300);
    await expect(page).toHaveURL(/\/overview\/\d+$/);
    await page.goto(`${server.url}/review`);
    await expect(page).toHaveURL(new RegExp(`^${server.url}/$`));
    expect(problems).toEqual([]);

    const revlog = await query<
        { ease: number; time: number; factor: number; type: number; cid: number }
    >("select ease, time, factor, type, cid from revlog order by id");
    expect(revlog.length).toBe(2);
    for (const entry of revlog) {
        expect(entry.ease).toBe(4);
        expect(entry.type).toBe(0);
        // FSRS stores difficulty here: 100 to 1100, and never the SM-2 default of 2500
        expect(entry.factor).toBeGreaterThanOrEqual(100);
        expect(entry.factor).toBeLessThanOrEqual(1100);
    }
    expect(revlog[0].time).toBeGreaterThanOrEqual(THINK_MILLIS);
    expect(revlog[0].time).toBeLessThan(30_000);

    const cards = await reopenQuery<{ type: number; data: string; ivl: number }>(
        "select type, data, ivl from cards",
    );
    expect(cards.length).toBe(2);
    for (const row of cards) {
        expect(row.type).toBe(2);
        expect(row.ivl).toBeGreaterThan(0);
        const memory = JSON.parse(row.data);
        expect(memory.s).toBeGreaterThan(0);
        expect(memory.d).toBeGreaterThanOrEqual(1);
        expect(memory.d).toBeLessThanOrEqual(10);
    }
});

/** A second query, for a server that's already stopped. */
function reopenQuery<T>(sql: string): T[] {
    const db = new DatabaseSync(collection.path, { readOnly: true });
    try {
        return db.prepare(sql).all() as T[];
    } finally {
        db.close();
    }
}

test("the keyboard drives the reviewer, and undo restores card and count", async ({ page }) => {
    await start({
        decks: [{ name: "Keys" }],
        notes: [BASIC("first", "a1", "Keys"), BASIC("second", "a2", "Keys")],
    });
    const problems = failOnPageProblems(page);
    await study(page, "Keys");
    const newCount = page.locator(".counts .new-count");
    await expect(card(page)).toContainText("first");
    await expect(newCount).toHaveText("2");

    // with the focus in the page
    await page.locator(".counts").click();
    await page.keyboard.press("Space");
    await expect(card(page)).toContainText("a1");
    await page.keyboard.press("4");
    await expect(card(page)).toContainText("second");
    await expect(newCount).toHaveText("1");

    await page.keyboard.press("u");
    await expect(card(page)).toContainText("first");
    await expect(newCount).toHaveText("2");
    await expect(page.getByRole("status")).toContainText("undone");

    // with the focus in the card's frame
    await card(page).click();
    await page.keyboard.press("Enter");
    await expect(card(page)).toContainText("a1");
    await card(page).click();
    await page.keyboard.press("3");
    await expect(card(page)).toContainText("second");
    await card(page).click();
    await page.keyboard.press("u");
    await expect(card(page)).toContainText("first");

    // holding a key down mustn't rate a run of cards
    await page.keyboard.press("Space");
    await expect(card(page)).toContainText("a1");
    await page.keyboard.down("4");
    await page.keyboard.down("4");
    await page.keyboard.down("4");
    await page.keyboard.up("4");
    await expect(card(page)).toContainText("second");
    await page.waitForTimeout(200);
    await expect(card(page)).toContainText("second");
    expect(problems).toEqual([]);

    const revlog = await query<{ ease: number }>("select ease from revlog");
    expect(revlog).toEqual([{ ease: 4 }]);
});

test("each deck's options are its own, down to subdecks and new ones", async ({ page }) => {
    await start({
        decks: [{ name: "First" }, { name: "Second" }, { name: "Second::Sub" }],
        notes: [BASIC("one", "a", "First"), BASIC("two", "a", "Second"), BASIC("three", "a", "Second::Sub")],
    });
    const problems = failOnPageProblems(page);
    const open = async (name: string) => {
        await page.goto(`${server.url}/`);
        await page.locator(`tr[data-deck="${name}"] .options a`).click();
        const title = page.locator(".setting-title", { hasText: "New cards/day" });
        const input = page.locator(".row", { has: title }).locator("input").first();
        await expect(input).toBeVisible();
        return input;
    };
    await expect(await open("First")).toHaveValue("20");
    const input = await open("Second");
    await input.fill("33");
    await input.blur();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page).toHaveURL(/\/overview\//);

    await expect(await open("Second")).toHaveValue("33");
    await expect(await open("First")).toHaveValue("20");
    await expect(await open("Second::Sub")).toHaveValue("20");
    expect(problems).toEqual([]);

    const rows = await query<{ count: number }>("select count() as count from deck_config");
    const decks = await query<{ count: number }>("select count() as count from decks");
    expect(rows).toEqual(decks);
});

test("failing a card leaves it unsuspended and untagged, with no leech controls", async ({ page }) => {
    await start({
        decks: [{ name: "Lapses" }],
        notes: [{ ...BASIC("lapsing", "a", "Lapses"), due: true }, BASIC("next", "a", "Lapses")],
    });
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/`);
    await page.locator(`tr[data-deck="Lapses"] .options a`).click();
    await expect(page.locator(".setting-title", { hasText: "Relearning steps" })).toBeVisible();
    await expect(page.locator(".setting-title", { hasText: /leech/i })).toHaveCount(0);
    await page.goBack();

    await page.locator(`tr[data-deck="Lapses"] .name a`).click();
    await page.getByRole("button", { name: "Study Now" }).click();
    await expect(card(page)).toContainText(/lapsing|next/);
    if ((await card(page).textContent())!.includes("next")) {
        await page.keyboard.press("Space");
        await page.keyboard.press("3");
    }
    await expect(card(page)).toContainText("lapsing");
    await page.keyboard.press("Space");
    await expect(page.locator("button[data-rating='1']")).toBeVisible();
    await page.keyboard.press("1");
    await expect(card(page)).toContainText(/lapsing|next/);
    await expect(page.getByRole("status")).toHaveCount(0);
    expect(problems).toEqual([]);

    const rows = await query<{ queue: number; lapses: number; tags: string }>(
        "select c.queue, c.lapses, n.tags from cards c join notes n on c.nid = n.id where n.sfld = 'lapsing'",
    );
    expect(rows).toEqual([{ queue: expect.any(Number), lapses: 1, tags: "" }]);
    expect(rows[0].queue).toBeGreaterThanOrEqual(0);
});

test("media loads in the sandbox, and audio plays through the page", async ({ page }) => {
    await start({
        decks: [{ name: "Media" }],
        notes: [
            BASIC(
                `<img id="pic" src="pixel.png">[sound:sound file.wav]`,
                "[sound:sound file.wav]after",
                "Media",
            ),
        ],
    });
    const problems = failOnPageProblems(page);
    await study(page, "Media");

    const image = card(page).locator("#pic");
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);

    const replay = card(page).locator("a.replay-button");
    await expect(replay).toBeVisible();
    await page.locator(".counts").click();

    const audio = page.locator("audio");
    const played = () => audio.evaluate((a: HTMLAudioElement) => a.played.length);
    const src = () => audio.evaluate((a: HTMLAudioElement) => a.src);
    // autoplay may or may not be allowed, but after that click a key press must work
    await page.keyboard.press("r");
    await expect.poll(src).toMatch(/\/_frame\/[0-9a-f]+\/m\/sound%20file\.wav$/);
    await expect.poll(played).toBeGreaterThan(0);
    await expect.poll(() => audio.evaluate((a: HTMLAudioElement) => a.error)).toBeNull();

    // and so does the button in the card
    await audio.evaluate((a: HTMLAudioElement) => a.removeAttribute("src"));
    await replay.click();
    await expect.poll(src).toMatch(/sound%20file\.wav$/);
    expect(problems).toEqual([]);
});

test("cloze and type-answer cards compare what was typed", async ({ page }) => {
    const front = "<b>capital</b> of France";
    await start({
        decks: [{ name: "Types" }],
        notes: [
            { deck: "Types", notetype: "Basic (type in the answer)", fields: [front, "Paris"] },
            { deck: "Types", notetype: "Cloze", fields: ["The {{c1::sun}} is hot", ""] },
        ],
    });
    const problems = failOnPageProblems(page);
    await study(page, "Types");
    await expect(card(page)).toContainText("capital");

    const input = card(page).locator("#typeans");
    await expect(input).toBeFocused();
    await input.fill("Pariss");
    // typing a shortcut key there must not reveal or rate anything
    await input.press("1");
    await expect(input).toBeVisible();
    await input.fill("Pariss");
    await input.press("Enter");
    await expect(card(page).locator(".typeGood").first()).toBeVisible();
    await expect(card(page).locator("#typeans")).toHaveText(/Pariss/);
    await expect(card(page).locator(".typeBad, .typeMissed").first()).toBeVisible();
    await page.keyboard.press("3");

    await expect(card(page).locator(".cloze")).toHaveText("[...]");
    await expect(card(page)).toContainText("is hot");
    await page.locator(".counts").click();
    await page.keyboard.press("Space");
    await expect(card(page).locator(".cloze")).toHaveText("sun");
    expect(problems).toEqual([]);
});

test("MathJax typesets in the sandbox, while template scripts and inline handlers don't run", async ({ page }) => {
    await start({
        decks: [{ name: "Math" }],
        notes: [
            BASIC(
                String.raw`\(x^2\)<div id="scripted">untouched</div><script>
                    document.getElementById("scripted").textContent = "ran";
                </script>
                <img src="pixel.png" onload="document.getElementById('scripted').textContent = 'onload'">
                <a id="clicker" href="#" onclick="document.getElementById('scripted').textContent = 'onclick'">x</a>
                <a id="js-link" href="javascript:document.getElementById('scripted').textContent='link'">y</a>
                <svg onload="document.getElementById('scripted').textContent = 'svg'"></svg>`,
                "a",
                "Math",
            ),
        ],
    });
    // the frame's CSP reports what it stopped
    const problems = failOnPageProblems(page, [/Content Security Policy|script-src/]);
    await study(page, "Math");
    await expect(card(page).locator("mjx-container")).toBeVisible({ timeout: 20_000 });
    // the fonts come from the frame's own prefix too, and are fetched with CORS
    await expect
        .poll(() => card(page).evaluate(() => [...document.fonts].filter((f) => f.status === "loaded").length))
        .toBeGreaterThan(0);

    await card(page).locator("#clicker").click();
    await card(page).locator("#js-link").click();
    await expect(card(page).locator("#scripted")).toHaveText("untouched");
    expect(problems).toEqual([]);
});

test("the hint filter works without inline handlers", async ({ page }) => {
    await start({
        decks: [{ name: "Hints" }],
        notes: [{ deck: "Hints", notetype: "Hinted", fields: ["question", "the hint text", "a"] }],
        notetypes: [{
            name: "Hinted",
            fields: ["Front", "Extra", "Back"],
            front: "{{Front}}{{hint:Extra}}",
            back: "{{Back}}",
        }],
    });
    const problems = failOnPageProblems(page);
    await study(page, "Hints");
    const hint = card(page).locator("div.hint");
    await expect(hint).toBeHidden();
    await card(page).locator("a.hint").click();
    await expect(hint).toBeVisible();
    await expect(hint).toHaveText("the hint text");
    await expect(card(page).locator("a.hint")).toBeHidden();
    expect(problems).toEqual([]);
});

test("a malicious card can't run, or touch the API, the page or the review", async ({ page, request }) => {
    const seenByServer: { url: string; status: number }[] = [];
    page.on("response", (response) => {
        if (response.request().method() === "POST" && response.frame() !== page.mainFrame()) {
            seenByServer.push({ url: response.url(), status: response.status() });
        }
    });
    await start({
        decks: [{ name: "Evil" }],
        notes: [BASIC(ATTACK_SCRIPT, "the answer", "Evil"), BASIC("the next card", "x", "Evil")],
    });
    const dialogs: string[] = [];
    page.on("dialog", (dialog) => {
        dialogs.push(dialog.message());
        void dialog.dismiss();
    });
    await page.goto(server.url);
    await page.evaluate(() => {
        document.title = "SECRET PAGE TITLE";
    });
    await study(page, "Evil");

    // the script simply doesn't run
    const log = card(page).locator(`#${ATTACK_LOG}`);
    await expect(log).toHaveText("pending");
    await page.waitForTimeout(300);
    await expect(log).toHaveText("pending");

    await expect(page).toHaveURL(/\/review$/);
    await expect(page.getByRole("button", { name: "Show Answer" })).toBeVisible();
    await expect(page.locator(".counts .new-count")).toHaveText("2");
    expect(dialogs).toEqual([]);
    expect(seenByServer).toEqual([]);
    expect(await page.title()).not.toContain("threw");

    // If it somehow did run, the sandbox and the server are still in its way. A
    // sandboxed srcdoc frame has none of the card CSP, so use one to run the attack.
    // The reviewer's own CSP would apply to a srcdoc frame, so use a page without one.
    const plain = await page.context().newPage();
    await plain.goto(`${server.url}/graphs`);
    const attempts = await plain.evaluate((attack) => {
        return new Promise<unknown>((resolve) => {
            const frame = document.createElement("iframe");
            frame.setAttribute("sandbox", "allow-scripts");
            window.addEventListener("message", (event) => {
                if (event.source === frame.contentWindow && event.data.results) {
                    resolve(event.data.results);
                }
            });
            frame.srcdoc = attack;
            document.body.append(frame);
        });
    }, ATTACK_SCRIPT);
    console.log("the attack, without the card CSP:", attempts);
    expect(attempts).toMatchObject({
        parentDocument: expect.stringMatching(/^threw SecurityError/),
        topDocument: expect.stringMatching(/^threw SecurityError/),
        fetchAnswer: expect.stringMatching(/^threw TypeError/),
        xhr: "error",
        cookie: expect.stringMatching(/^threw SecurityError|^$/),
        storage: expect.stringMatching(/^threw SecurityError/),
        topNavigate: expect.stringMatching(/^threw/),
        popup: "null",
    });
    const refused = await plain.evaluate(() => {
        return new Promise<unknown>((resolve) => {
            const frame = document.createElement("iframe");
            frame.setAttribute("sandbox", "allow-scripts");
            window.addEventListener("message", (event) => {
                if (event.source === frame.contentWindow && !event.data.results) {
                    resolve(event.data);
                }
            });
            frame.srcdoc = `<script>
                const url = ${JSON.stringify(location.origin)} + "/_anki/getQueuedCards";
                Promise.all([
                    fetch(url, {
                        method: "POST",
                        mode: "no-cors",
                        headers: {"Content-Type": "text/plain"},
                        body: "",
                    }).then(r => r.type, e => "threw " + e.name),
                    fetch(url, {
                        method: "POST",
                        headers: {"Content-Type": "application/binary"},
                        body: new Uint8Array(),
                    }).then(r => r.status, e => "threw " + e.name),
                    fetch(url.replace("getQueuedCards", "frameBase"))
                        .then(r => r.status, e => "threw " + e.name),
                ]).then((r) => parent.postMessage(r, "*"));
            </script>`;
            document.body.append(frame);
        });
    });
    await plain.close();
    console.log("sandboxed frame without the card CSP:", refused);
    // The server's Origin check or its unanswered preflight fails each of them.
    expect(refused).toEqual(["threw TypeError", "threw TypeError", "threw TypeError"]);

    // Same requests with the exact headers a browser sends from a sandboxed frame
    for (
        const headers of [
            { Origin: "null", "Sec-Fetch-Site": "cross-site", "Content-Type": "application/binary" },
            { Origin: "null", "Content-Type": "application/binary" },
            { "Sec-Fetch-Site": "cross-site", "Content-Type": "application/binary" },
        ] as Record<string, string>[]
    ) {
        const response = await request.post(`${server.url}/_anki/undo`, { headers, data: Buffer.alloc(0) });
        expect(response.status()).toBe(403);
    }

    // and it had no effect on the data
    await page.keyboard.press("Space");
    await expect(page.locator("button[data-rating='3']")).toBeVisible();
    const revlog = await query("select id from revlog");
    expect(revlog).toEqual([]);
});

// Card scripts don't run, but a meta refresh needs none: it's the remaining way for
// content to move its frame.
test("a card can't escape by navigating its frame", async ({ page }) => {
    await start({
        decks: [{ name: "Leaver" }],
        notes: [
            BASIC(`<meta http-equiv="refresh" content="0;url=https://example.com/"><div>stays</div>`, "a", "Leaver"),
            BASIC(`<meta http-equiv="refresh" content="0;url=?again">`, "b", "Leaver"),
            BASIC("plain", "c", "Leaver"),
        ],
    });
    const problems = failOnPageProblems(page, [/frame-src|Refused to frame|example\.com/]);
    const external: string[] = [];
    page.on("request", (r) => {
        if (r.url().includes("example.com")) {
            external.push(r.url());
        }
    });
    const newCount = page.locator(".counts .new-count");
    /** Waits at each step, as the reviewer ignores keys while it's busy. */
    const answerGood = async () => {
        await page.locator(".counts").click();
        await page.keyboard.press("Space");
        await expect(page.locator("button[data-rating='3']")).toBeVisible();
        await page.keyboard.press("3");
    };

    await study(page, "Leaver");
    await expect(newCount).toHaveText("3");
    // blocked by the page's frame-src, which still counts as trying
    await expect(card(page)).toContainText("kept trying to leave", { timeout: 10_000 });
    await expect(page).toHaveURL(/\/review$/);
    expect(external).toEqual([]);

    await answerGood();
    // The same text as the last card, so the count is what shows this is a new one.
    await expect(newCount).toHaveText("2");
    // this one reloads its own frame, and is replaced each time until it's dropped
    await expect(card(page)).toContainText("kept trying to leave", { timeout: 10_000 });

    await answerGood();
    await expect(newCount).toHaveText("1");
    await expect(card(page)).toContainText("plain");
    expect(problems).toEqual([]);
});

test("a deck description can't run scripts", async ({ page }) => {
    const xss = `<img src=x onerror="parent.document.title='hacked'; window.hacked=1">`
        + `<script>window.hacked=1;parent.document.title='hacked'</script>`
        + `<svg onload="window.hacked=1"></svg><a id="evil" href="javascript:window.hacked=1">link</a>`
        + `<b id="safe">kept</b>`;
    await start({
        decks: [
            { name: "Html", description: xss },
            { name: "Md", description: `**bold** ${xss}`, markdown: true },
            { name: "Done", description: `**kept** ${xss}`, markdown: true },
        ],
        notes: [BASIC("q", "a", "Html"), BASIC("q", "a", "Md")],
    });
    const problems = failOnPageProblems(page, [
        /Content Security Policy|404|ERR_BLOCKED/,
    ]);

    for (const name of ["Html", "Md"]) {
        await openDeck(page, name);
        const description = page.frameLocator(".description iframe");
        await expect(description.locator(name === "Md" ? "strong" : "#safe")).toBeVisible();
        await page.waitForTimeout(200);
        expect(await page.evaluate(() => "hacked" in window)).toBe(false);
        expect(await page.title()).not.toBe("hacked");
        expect(
            await description.locator("body").evaluate(() => "hacked" in window),
        ).toBe(false);
        await expect(page.locator("main.overview img, main.overview script")).toHaveCount(0);
    }
    const sandbox = await page.locator(".description iframe").getAttribute("sandbox");
    expect(sandbox).toBe("allow-scripts");

    // A deck with nothing left shows it too.
    await openDeck(page, "Done");
    await expect(page.locator(".finished")).toBeVisible();
    const description = page.frameLocator(".description iframe");
    await expect(description.locator("strong")).toHaveText("kept");
    await page.waitForTimeout(200);
    expect(await page.evaluate(() => "hacked" in window)).toBe(false);
    await expect(page.locator(".description img")).toHaveCount(0);
    expect(problems).toEqual([]);
});

test("the theme follows the browser's setting, in pages and cards", async ({ browser }) => {
    await start({
        decks: [{ name: "Dark", description: "<i>described</i>" }],
        notes: [BASIC("question", "answer", "Dark")],
    });
    const open = async (scheme: "light" | "dark") => {
        const context = await browser.newContext({ colorScheme: scheme });
        const page = await context.newPage();
        const problems = failOnPageProblems(page);
        await page.goto(server.url);
        return { context, page, problems };
    };
    /** Stylesheets arrive with the page's modules, which can be after the load event. */
    const rendered = (page: Page) => page.locator("main.decks table").waitFor();
    const background = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const font = (page: Page) => page.evaluate(() => getComputedStyle(document.body).fontFamily);

    const light = await open("light");
    // a script in the head sets this, so it needs nothing else to have loaded
    expect(await light.page.evaluate(() => document.documentElement.className)).not.toContain("night-mode");
    await rendered(light.page);
    expect(await font(light.page)).toContain("sans-serif");
    expect(await font(light.page)).not.toMatch(/^"?Times/);
    const lightBackground = await background(light.page);

    const dark = await open("dark");
    expect(await dark.page.evaluate(() => document.documentElement.className)).toContain("night-mode");
    await rendered(dark.page);
    expect(await background(dark.page)).not.toBe(lightBackground);

    await dark.page.emulateMedia({ colorScheme: "light" });
    await expect.poll(() => background(dark.page)).toBe(lightBackground);
    await dark.page.emulateMedia({ colorScheme: "dark" });
    await expect.poll(() => background(dark.page)).not.toBe(lightBackground);

    await study(dark.page, "Dark");
    const body = card(dark.page).locator("xpath=ancestor::body");
    await expect(body).toHaveClass(/\bnightMode\b/);
    await expect(body).toHaveClass(/\bnight_mode\b/);
    await expect(body).toHaveClass(/\bcard1\b/);
    const cardBackground = await body.evaluate((b) => getComputedStyle(b).backgroundColor);
    expect(cardBackground).not.toBe("rgb(255, 255, 255)");

    await study(light.page, "Dark");
    const lightBody = card(light.page).locator("xpath=ancestor::body");
    await expect(card(light.page)).toContainText("question");
    await expect(lightBody).not.toHaveClass(/nightMode/);
    await expect(lightBody).toHaveClass(/\bcard1\b/);

    expect(light.problems).toEqual([]);
    expect(dark.problems).toEqual([]);
    await light.context.close();
    await dark.context.close();
});

test("deck options reports a slow save and returns to the deck", async ({ page }) => {
    await start({ decks: [{ name: "Opts" }], notes: [BASIC("q", "a", "Opts")] });
    const problems = failOnPageProblems(page);
    await openDeck(page, "Opts");
    await page.getByRole("link", { name: "Options" }).click();
    const title = page.locator(".setting-title", { hasText: "New cards/day" });
    const input = page.locator(".row", { has: title }).locator("input").first();
    await input.fill("37");
    await input.blur();

    let release = () => undefined as void;
    const held = new Promise<void>((resolve) => (release = resolve));
    await page.route("**/_anki/updateDeckConfigs", async (route) => {
        await held;
        await route.continue();
    });
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByRole("status")).toContainText("Saving");
    release();
    await expect(page).toHaveURL(/\/overview\/\d+/);
    await expect(page.getByRole("status")).toContainText("saved");

    // the reviewer doesn't run custom scheduling, and its setting is left alone
    await page.getByRole("link", { name: "Options" }).click();
    await expect(page.locator(".row", { has: title }).locator("input").first()).toHaveValue("37");
    await expect(page.getByText("Custom scheduling")).toHaveCount(0);
    expect(problems).toEqual([]);
});

test("the graphs page doesn't break without a card browser", async ({ page }) => {
    await start({ notes: [BASIC("q", "a")] });
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/graphs`);
    const clickable = page.locator(".graph svg .clickable, svg rect.clickable").first();
    if (await clickable.count()) {
        await clickable.click({ force: true });
    }
    await expect(page.getByRole("link", { name: "Decks" })).toBeVisible();
    expect(problems).toEqual([]);
});

/** Not a check: leaves pictures behind for a person to look at. */
test("screenshots", async ({ browser }: { browser: Browser }) => {
    const dir = process.env.EVOLANKI_SHOTS;
    test.skip(!dir, "EVOLANKI_SHOTS is not set");
    await start({
        decks: [
            { name: "Languages" },
            { name: "Languages::French" },
            { name: "Languages::German" },
            { name: "Science", description: "<b>Physics</b> and chemistry, from the textbook." },
        ],
        notes: [
            { deck: "Science", notetype: "Basic", fields: [`What is \\(E=mc^2\\)?`, "Mass-energy equivalence"] },
            { deck: "Science", notetype: "Basic", fields: ["Second", "Card"] },
            BASIC("bonjour", "hello", "Languages::French"),
            { ...BASIC("merci", "thank you", "Languages::French"), due: true },
            BASIC("danke", "thank you", "Languages::German"),
        ],
    });
    const viewports = {
        desktop: { viewport: { width: 1100, height: 700 } },
        mobile: { ...devices["Pixel 7"] },
    };
    for (const scheme of ["light", "dark"] as const) {
        for (const [size, options] of Object.entries(viewports)) {
            const context = await browser.newContext({ ...options, colorScheme: scheme });
            const page = await context.newPage();
            const shot = (name: string) => page.screenshot({ path: join(dir!, `${size}-${name}-${scheme}.png`) });
            await page.goto(server.url);
            await page.locator(`tr[data-deck="Languages"]`).waitFor();
            if (await page.locator(`tr[data-deck="French"]`).count() === 0) {
                await page.locator(`tr[data-deck="Languages"]`).getByRole("button").click();
                await page.locator(`tr[data-deck="French"]`).waitFor();
            }
            await shot("decks");
            await openDeck(page, "Science");
            await expect(page.frameLocator(".description iframe").locator("b")).toBeVisible();
            await page.waitForTimeout(100);
            await shot("overview");
            await page.getByRole("button", { name: "Study Now" }).click();
            await expect(card(page).locator("mjx-container")).toBeVisible({ timeout: 20_000 });
            await page.waitForTimeout(200);
            await shot("question");
            await page.getByRole("button", { name: "Show Answer" }).click();
            await expect(card(page)).toContainText("Mass-energy");
            await expect(card(page).locator("mjx-container")).toBeVisible();
            await page.waitForTimeout(200);
            await shot("answer");
            await page.goto(`${server.url}/deck-options/1`);
            await page.locator(".setting-title", { hasText: "New cards/day" }).first().waitFor();
            await page.waitForTimeout(100);
            await shot("options");
            await page.goto(`${server.url}/graphs`);
            await page.locator("svg").first().waitFor();
            await page.waitForTimeout(300);
            await shot("stats");
            await context.close();
        }
    }
});
