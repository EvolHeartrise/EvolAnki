// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Playwright runs from the repo root, where its config lives.
const REPO = process.cwd();
const BINARY = join(REPO, "out/rust/debug/evolanki-server");
const WEB_ROOT = join(REPO, "out/qt/_aqt/data/web");

export class Server {
    private constructor(private child: ChildProcess, readonly port: number) {}

    get url(): string {
        return `http://127.0.0.1:${this.port}`;
    }

    /** With no seed file, a new collection gets the demo content. */
    static start(collection: string, seedFile?: string): Promise<Server> {
        if (!existsSync(BINARY)) {
            throw new Error(`${BINARY} is missing; see docs/web-server.md`);
        }
        const child = spawn(BINARY, [
            "--collection",
            collection,
            "--create",
            ...(seedFile ? ["--seed-file", seedFile] : ["--seed-demo"]),
            "--port",
            "0",
            "--web-root",
            WEB_ROOT,
        ], { stdio: ["ignore", "pipe", "inherit"] });
        return new Promise((resolve, reject) => {
            child.once("exit", (code) => reject(new Error(`server exited: ${code}`)));
            child.stdout!.setEncoding("utf8").on("data", (text: string) => {
                const match = text.match(/Listening on http:\/\/127\.0\.0\.1:(\d+)/);
                if (match) {
                    child.removeAllListeners("exit");
                    resolve(new Server(child, Number(match[1])));
                }
            });
        });
    }

    /** SIGTERM; rejects unless the collection closed and the exit was clean. */
    stop(): Promise<void> {
        let output = "";
        this.child.stdout!.on("data", (text: string) => (output += text));
        return new Promise((resolve, reject) => {
            this.child.once("exit", (code) => {
                if (code === 0 && output.includes("Collection closed")) {
                    resolve();
                } else {
                    reject(new Error(`unclean exit ${code}: ${output}`));
                }
            });
            this.child.kill("SIGTERM");
        });
    }

    kill(): void {
        this.child.kill("SIGKILL");
    }
}

export function tempCollection(): { path: string; cleanup: () => void } {
    const dir = mkdtempSync(join(tmpdir(), "evolanki-web-"));
    return {
        path: join(dir, "col.anki2"),
        cleanup: () => rmSync(dir, { recursive: true, force: true }),
    };
}

/** Send a request verbatim, as browsers and fetch() normalise paths and refuse to set Host. */
export function rawRequest(port: number, request: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const socket = net.connect(port, "127.0.0.1", () => socket.write(request));
        let response = "";
        socket.setEncoding("utf8");
        socket.on("data", (data) => (response += data));
        socket.on("end", () => resolve(response));
        socket.on("error", reject);
    });
}
