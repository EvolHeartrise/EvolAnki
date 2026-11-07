// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Usage: node esbuild.mjs ENTRYPOINT OUTFILE bundle|transform
// "bundle" bundles the entrypoint's imports; "transform" only converts the
// one file. Both minify.

import { build } from "esbuild";
import { argv } from "process";

const [_node, _script, entrypoint, outfile, mode] = argv;
const bundle = mode === "bundle";

build({
    bundle,
    entryPoints: [entrypoint],
    outfile,
    minify: true,
    preserveSymlinks: true,
    target: ["es6", "chrome77"],
}).catch(() => process.exit(1));
