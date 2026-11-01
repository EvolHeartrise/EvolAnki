// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { expect, test } from "./fixtures";

test("FSRS controls are always shown, with no on/off toggle", async ({ page }) => {
    await page.goto("/deck-options/1");

    await expect(page.getByRole("button", { name: "FSRS Parameters", exact: true })).toBeVisible();
    await expect(page.getByRole("checkbox", { name: /^FSRS\b/ })).toHaveCount(0);
});

test("FSRS parameter unlock timing can be configured by the host", async ({ page }) => {
    await page.clock.install();
    await page.goto("/deck-options/1");

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
