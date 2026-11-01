// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Browser tests for evolanki-server. Unlike playwright.config.ts, these need no
// Python: each test starts the server itself, so it can also restart it.

import { defineConfig } from "@playwright/test";

export default defineConfig({
    testDir: "./ts/tests/web",
    outputDir: "./out/e2e-web-results/",
    fullyParallel: false,
    workers: 1,
    forbidOnly: !!process.env.CI,
    retries: 0,
    reporter: "list",
    use: {
        trace: "retain-on-failure",
        screenshot: "only-on-failure",
    },
});
