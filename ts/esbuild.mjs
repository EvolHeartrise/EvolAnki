// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Usage: node esbuild.mjs ENTRYPOINT OUTFILE bundle|transform
// "bundle" bundles the entrypoint's imports, minifying for release builds;
// "transform" only converts the one file, always minified.

import { build } from "esbuild";
import { argv, env } from "process";

const [_node, _script, entrypoint, outfile, mode] = argv;
const bundle = mode === "bundle";

build({
    bundle,
    entryPoints: [entrypoint],
    outfile,
    minify: bundle ? Boolean(env.RELEASE) : true,
    sourcemap: bundle && env.SOURCEMAP ? "inline" : false,
    preserveSymlinks: true,
    target: ["es6", "chrome77"],
}).catch(() => process.exit(1));
