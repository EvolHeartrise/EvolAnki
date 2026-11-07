// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Usage: node genproto.mjs DESCRIPTORS OUTDIR
// Generates the TypeScript protobuf code with protoc-gen-es, from the
// descriptor set that rslib/proto's build script writes. Doing what protoc
// does for a plugin means no protoc is needed: the plugin reads a
// CodeGeneratorRequest on stdin, and writes a CodeGeneratorResponse.

import { CodeGeneratorRequest, CodeGeneratorResponse, FileDescriptorSet } from "@bufbuild/protobuf";
import { execFileSync } from "child_process";
import { mkdirSync, readFileSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { argv, exit } from "process";

const [descriptors, outDir] = argv.slice(2);

const set = FileDescriptorSet.fromBinary(readFileSync(descriptors));
const request = new CodeGeneratorRequest({
    protoFile: set.file,
    // not any google/protobuf files they import
    fileToGenerate: set.file.map((file) => file.name).filter((name) => name.startsWith("anki/")),
});
const output = execFileSync("node_modules/.bin/protoc-gen-es", {
    input: request.toBinary(),
    maxBuffer: 1 << 30,
});
const response = CodeGeneratorResponse.fromBinary(output);
if (response.error) {
    console.error(`protoc-gen-es: ${response.error}`);
    exit(1);
}
for (const file of response.file) {
    const path = join(outDir, file.name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, file.content);
}
