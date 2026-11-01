// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { dirname, join } from "node:path";

import { chromium, expect, type Page, test } from "@playwright/test";

import { GetQueuedCardsRequest, QueuedCards } from "../../../out/ts/lib/generated/anki/scheduler_pb";
import { rawRequest, Server, tempCollection } from "./server";

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
    await page.getByRole("button", { name: "Optimize Current Preset" }).click();
    expect((await computed).ok()).toBe(true);
    expect((await polled).ok()).toBe(true);
    // too few reviews to optimize, so it reports that its parameters are already best
    await expect.poll(() => messages.length).toBe(1);
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

test("card info renders", async ({ page, request }) => {
    const problems = failOnPageProblems(page);
    const response = await request.post(`${server.url}/_anki/getQueuedCards`, {
        headers: { "Content-Type": "application/binary" },
        data: Buffer.from(new GetQueuedCardsRequest({ fetchLimit: 1 }).toBinary()),
    });
    expect(response.status()).toBe(200);
    const queued = QueuedCards.fromBinary(await response.body());
    const cardId = queued.cards[0].card!.id;

    await page.goto(`${server.url}/card-info/${cardId}`);
    await expect(page.getByText("Card ID")).toBeVisible();
    await expect(page.getByRole("cell", { name: String(cardId) }).first()).toBeVisible();
    expect(problems).toEqual([]);
});

test("congrats renders without Qt-only links", async ({ page }) => {
    const problems = failOnPageProblems(page);
    await page.goto(`${server.url}/congrats`);
    await expect(page.locator(".congrats h1")).toBeVisible();
    await expect(page.locator(".congrats a[href^='javascript']")).toHaveCount(0);
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
    expect(await statusOf("openFilePicker")).toBe(501);
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
