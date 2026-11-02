// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { dirname, join } from "node:path";

import { chromium, expect, type Page, test } from "@playwright/test";

import { lanAddress, rawRequest, Server, tempCollection } from "./server";

const NEW_CARDS_LABEL = "New cards/day";

let collection: ReturnType<typeof tempCollection>;
let server: Server;

test.beforeEach(async () => {
    collection = tempCollection();
    server = await Server.start(collection.path);
});

test.afterEach(async () => {
    server.kill();
    collection.cleanup();
});

/** Fail the test on anything the user would see go wrong. */
function failOnPageProblems(page: Page): string[] {
    const problems: string[] = [];
    page.on("dialog", (dialog) => {
        problems.push(`dialog: ${dialog.message()}`);
        void dialog.dismiss();
    });
    page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
    page.on("console", (message) => {
        if (message.type() === "error") {
            problems.push(`console: ${message.text()}`);
        }
    });
    page.on("response", (response) => {
        if (response.status() >= 400) {
            problems.push(`${response.status()}: ${response.url()}`);
        }
    });
    return problems;
}

function newCardsInput(page: Page) {
    const title = page.locator(".setting-title", { hasText: NEW_CARDS_LABEL });
    return page.locator(".row", { has: title }).locator("input").first();
}

test("the favicon is served", async ({ page }) => {
    const response = await page.goto(`${server.url}/favicon.ico`);
    expect(response?.status()).toBe(200);
});

test("FSRS parameter unlock timing can be configured by the host", async ({ page }) => {
    await page.clock.install();
    await page.goto(`${server.url}/deck-options/1`);

    const parameters = page.getByRole("button", { name: "FSRS Parameters", exact: true });
    const input = parameters.locator("textarea");
    await expect(input).toBeDisabled();
    await page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));

    async function setTimeoutMs(ms: number): Promise<void> {
        await page.evaluate((ms) => (window as any).anki.setParameterUnlockClickTimeoutMs(ms), ms);
    }

    async function clickThreeTimes(interval: number): Promise<void> {
        await parameters.click();
        await page.clock.runFor(interval);
        await parameters.click();
        await expect(input).toBeDisabled();
        await page.clock.runFor(interval);
        await parameters.click();
    }

    const defaultMs = await page.evaluate(() => (window as any).anki.defaultParameterUnlockClickTimeoutMs);
    expect(defaultMs).toBe(500);

    // A longer timeout set by the host applies to the mounted controls.
    await setTimeoutMs(1000);
    await clickThreeTimes(750);
    await expect(input).toBeEnabled();

    // Host preferences last for this page only; a fresh page starts at the default.
    await page.reload();
    await expect(input).toBeDisabled();
    await clickThreeTimes(750);
    await expect(input).toBeDisabled();

    // Changes made after a partial attempt also apply, without changing the three-click gate.
    await page.clock.runFor(defaultMs + 1);
    await setTimeoutMs(2000);
    await clickThreeTimes(1250);
    await expect(input).toBeEnabled();
});

test("deck options saves, and the change survives a restart", async ({ page }) => {
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/deck-options/1`);
    const input = newCardsInput(page);
    await expect(input).toBeVisible();
    await expect(input).toHaveValue("20");

    await input.fill("37");
    await input.blur();
    const saved = page.waitForResponse((r) => r.url().endsWith("/_anki/updateDeckConfigs"));
    await page.getByRole("button", { name: "Save", exact: true }).click();
    expect((await saved).status()).toBe(200);

    await expect(page).toHaveURL(/\/overview\/1$/);
    await page.goto(`${server.url}/deck-options/1`);
    await expect(newCardsInput(page)).toHaveValue("37");

    await server.stop();
    server = await Server.start(collection.path);
    await page.goto(`${server.url}/deck-options/1`);
    await expect(newCardsInput(page)).toHaveValue("37");

    expect(problems).toEqual([]);
});

test("FSRS optimize runs with progress polling", async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    const messages: string[] = [];
    page.on("dialog", (dialog) => {
        messages.push(dialog.message());
        void dialog.accept();
    });
    await page.goto(`${server.url}/deck-options/1`);
    const polled = page.waitForResponse((r) => r.url().endsWith("/_anki/latestProgress"));
    const computed = page.waitForResponse((r) => r.url().endsWith("/_anki/computeFsrsParams"));
    await page.getByRole("button", { name: "Optimize This Deck" }).click();
    expect((await computed).ok()).toBe(true);
    expect((await polled).ok()).toBe(true);
    // too few reviews to optimize, so it reports that its parameters are already best
    await expect.poll(() => messages.length).toBe(1);
    expect(problems).toEqual([]);
});

test("deck options has settings for this deck", async ({ page }) => {
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/deck-options/1`);
    await expect(page.getByRole("heading", { name: "Default", exact: true })).toBeVisible();
    await expect(newCardsInput(page)).toBeVisible();

    // the daily limits have a regular value, and one for today
    const everyDay = page.getByRole("button", { name: "Every day", exact: true });
    const today = page.getByRole("button", { name: "Today only", exact: true });
    await expect(everyDay).toHaveCount(2);
    await expect(today).toHaveCount(2);
    // and the optimizer works on this deck
    await expect(page.getByRole("button", { name: "Optimize This Deck" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Optimize All Decks" })).toBeVisible();
    expect(problems).toEqual([]);
});

test("a today-only limit is saved separately from the regular one", async ({ page }) => {
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/deck-options/1`);
    const input = newCardsInput(page);
    await expect(input).toHaveValue("20");
    await page.getByRole("button", { name: "Today only", exact: true }).first().click();
    await input.fill("3");
    await input.blur();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page).toHaveURL(/\/overview\/1/);

    await page.goto(`${server.url}/deck-options/1`);
    await expect(newCardsInput(page)).toHaveValue("3");
    await page.getByRole("button", { name: "Every day", exact: true }).first().click();
    await expect(newCardsInput(page)).toHaveValue("20");
    expect(problems).toEqual([]);
});

test("saving from the simulator changes what the page shows", async ({ page }) => {
    const problems: string[] = [];
    page.on("pageerror", (error) => problems.push(error.message));
    page.on("dialog", (dialog) => void dialog.accept());
    await page.goto(`${server.url}/deck-options/1`);
    await expect(newCardsInput(page)).toHaveValue("20");
    await page.getByRole("button", { name: "FSRS Simulator (Experimental)" }).click();
    const modal = page.locator(".modal.show");
    await expect(modal).toBeVisible();
    const inModal = modal.locator(".row", { has: page.locator(".setting-title", { hasText: NEW_CARDS_LABEL }) })
        .last().locator("input").first();
    await inModal.fill("7");
    await inModal.blur();
    await modal.getByRole("button", { name: "Save Changes to Deck" }).click();
    await modal.getByRole("button", { name: "Close" }).first().click();
    await expect(newCardsInput(page)).toHaveValue("7");

    const saved = page.waitForResponse((r) => r.url().endsWith("/_anki/updateDeckConfigs"));
    await page.getByRole("button", { name: "Save", exact: true }).click();
    expect((await saved).status()).toBe(200);
    await expect(page).toHaveURL(/\/overview\/1/);
    await page.goto(`${server.url}/deck-options/1`);
    await expect(newCardsInput(page)).toHaveValue("7");
    expect(problems).toEqual([]);
});

test("graphs renders with data", async ({ page }) => {
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/graphs`);
    // 5 demo reviews were answered today
    await expect(page.getByText(/Studied \P{L}*5\P{L}* cards/u)).toBeVisible();
    await expect(page.locator("svg").first()).toBeVisible();
    expect(problems).toEqual([]);
});

test("methods off the allow-list are refused", async ({ page }) => {
    await page.goto(`${server.url}/graphs`);
    const statusOf = (method: string) =>
        page.evaluate(async (method) => {
            const response = await fetch(`/_anki/${method}`, {
                method: "POST",
                headers: { "Content-Type": "application/binary" },
            });
            return response.status;
        }, method);
    expect(await statusOf("closeCollection")).toBe(404);
    expect(await statusOf("getAbsoluteMediaPath")).toBe(404);
    expect(await statusOf("suspendCards")).toBe(404);
    expect(await statusOf("getGraphPreferences")).toBe(200);
});

test("another site's page can't drive the API", async ({ page }) => {
    const evil = createServer((_request, response) => {
        response.setHeader("Content-Type", "text/html");
        response.end("<h1>evil</h1>");
    });
    await new Promise<void>((resolve) => evil.listen(0, "127.0.0.1", resolve));
    const evilPort = (evil.address() as AddressInfo).port;
    let refused: unknown[];
    try {
        await page.goto(`http://127.0.0.1:${evilPort}/`);
        refused = await page.evaluate(async (target) => {
            const attempt = (headers: Record<string, string>, mode: RequestMode) =>
                fetch(`${target}/_anki/setConfigJson`, { method: "POST", headers, mode, body: "x" })
                    .then((r) => r.status || "opaque", (e) => String(e.name));
            return [
                await attempt({ "Content-Type": "application/binary" }, "cors"),
                await attempt({ "Content-Type": "text/plain" }, "cors"),
                await attempt({ "Content-Type": "text/plain" }, "no-cors"),
            ];
        }, server.url);
    } finally {
        evil.close();
    }
    // Each fails in the browser: preflight refused, CORS, or the CORP response header.
    expect(refused).toEqual(["TypeError", "TypeError", "TypeError"]);

    // And the simple requests never reach a handler: the server refuses that Origin.
    const reply = await rawRequest(
        server.port,
        `POST /_anki/setConfigJson HTTP/1.1\r\nHost: 127.0.0.1:${server.port}\r\n`
            + `Origin: http://127.0.0.1:${evilPort}\r\nContent-Type: application/binary\r\n`
            + "Connection: close\r\nContent-Length: 0\r\n\r\n",
    );
    expect(reply).toMatch(/^HTTP\/1\.1 403/);
});

test("DNS rebinding is refused", async () => {
    const browser = await chromium.launch({
        args: ["--host-resolver-rules=MAP evil.test 127.0.0.1"],
    });
    try {
        const page = await browser.newPage();
        const response = await page.goto(`http://evil.test:${server.port}/graphs`);
        expect(response!.status()).toBe(403);
        expect(await page.textContent("body")).toContain("host not allowed");
    } finally {
        await browser.close();
    }
});

test("reachable and usable through this machine's own LAN address", async ({ page }) => {
    const ip = lanAddress();
    test.skip(!ip, "this machine has no network address");
    await server.stop();
    server = await Server.start(collection.path, undefined, ["--host", "0.0.0.0"]);
    expect(server.startOutput).toContain(`http://${ip}:${server.port}`);
    expect(server.startOutput).toMatch(/\.local:\d+/);
    expect(server.startOutput).toMatch(/WARNING.*no login.*anyone on your network/);

    const problems = failOnPageProblems(page);
    const lan = `http://${ip}:${server.port}`;
    await page.goto(lan);
    await page.locator(`tr[data-deck="Default"] .name a`).click();
    await page.getByRole("button", { name: "Study Now" }).click();
    // over plain HTTP this isn't a secure context, and all of it must work regardless
    expect(await page.evaluate(() => window.isSecureContext)).toBe(false);
    const card = page.frameLocator("iframe.sandboxed-frame").locator("#qa");
    await expect(card).toContainText("Question");
    await page.getByRole("button", { name: "Show Answer" }).click();
    await expect(card).toContainText("Answer");
    await page.locator("button[data-rating='3']").click();
    await expect(page).toHaveURL(/\/review$/);
    expect(problems).toEqual([]);

    // A page on another origin (here, the same server by another name) is refused,
    // with the browser's own headers, even though it's loopback and this one is private.
    await page.goto(`${server.url}/graphs`);
    const refused = await page.evaluate(async (target) => {
        const response = await fetch(`${target}/_anki/getGraphPreferences`, {
            method: "POST",
            mode: "no-cors",
            headers: { "Content-Type": "text/plain" },
            body: "x",
        }).then((r) => r.status, (e) => String(e.name));
        return response;
    }, lan);
    expect(refused).not.toBe(200);
    const reply = await rawRequest(
        server.port,
        `POST /_anki/undo HTTP/1.1\r\nHost: ${ip}:${server.port}\r\nOrigin: http://evil.example\r\n`
            + "Content-Type: application/binary\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
    );
    expect(reply).toMatch(/^HTTP\/1\.1 403/);
    const rebound = await rawRequest(
        server.port,
        `GET /graphs HTTP/1.1\r\nHost: 203.0.113.9:${server.port}\r\nConnection: close\r\n\r\n`,
    );
    expect(rebound).toMatch(/^HTTP\/1\.1 403/);
});

test("media paths can't escape the media folder", async ({ page }) => {
    const media = collection.path.replace(/\.anki2$/, ".media");
    mkdirSync(media, { recursive: true });
    writeFileSync(join(media, "hello.txt"), "hello");
    writeFileSync(join(dirname(media), "secret.txt"), "top secret");

    const response = await page.goto(`${server.url}/hello.txt`);
    expect(await response!.text()).toBe("hello");
    expect(response!.headers()["content-security-policy"]).toContain("script-src 'none'");

    for (
        const path of [
            "/../secret.txt",
            "/%2e%2e/secret.txt",
            "/..%2fsecret.txt",
            "/%2e%2e%2fsecret.txt",
            "/..%5csecret.txt",
            "/_anki/%2e%2e%2f%2e%2e/secret.txt",
        ]
    ) {
        const reply = await rawRequest(
            server.port,
            `GET ${path} HTTP/1.1\r\nHost: 127.0.0.1:${server.port}\r\nConnection: close\r\n\r\n`,
        );
        expect(reply, path).toMatch(/^HTTP\/1\.1 403/);
        expect(reply, path).not.toContain("top secret");
    }
});
