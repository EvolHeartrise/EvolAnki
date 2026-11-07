# The build system

## Basic use

See [development.md](development.md).

## Architecture

The build/ folder is made up of 3 packages:

- build/configure defines the actions and inputs/outputs of the build graph -
  this is where you add new build steps or modify existing ones. The defined
  actions are converted at build time to a build.ninja file that Ninja executes.
- build/ninja_gen is a library for writing a build.ninja file, and includes
  various rules like "build a Rust crate". It also downloads, checksums and
  extracts dependencies as part of the build.
- build/runner serves a number of purposes:
    - it's the entrypoint to the build process, taking care of generating
      the build file and then invoking Ninja
    - it wraps executable invocations in the build file, swallowing their output
      if they exit successfully
    - it provides a few helpers for multi-step processes.

The build supports Linux only, on x86-64 or arm64. Node and protoc are
downloaded as part of the build. To use your own, define NODE_BINARY,
YARN_BINARY and/or PROTOC_BINARY. To update the pinned versions, change the
URLs and SHA-256 checksums in build/ninja_gen/src/node.rs and protobuf.rs.

`rslib/proto` generates the Rust and TypeScript interfaces to the backend,
which `./ninja` writes to `out/`. The default target,
`web`, builds the folder that the server serves, `out/web`.

## Tracing build problems

If you run into trouble with the build process:

- You can see the executed commands with e.g. `./ninja web -v`
- You can see the output of successful commands by defining N2_OUTPUT_SUCCESS=1
- You can see what's triggering a rebuild of a target with e.g.
  `./ninja web -d explain`.
- You can browse the build graph via e.g. `./ninja -- -t browse web`
