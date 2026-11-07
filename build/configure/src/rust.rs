// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anyhow::Result;
use ninja_gen::cargo::CargoBuild;
use ninja_gen::cargo::CargoClippy;
use ninja_gen::cargo::CargoFormat;
use ninja_gen::cargo::CargoTest;
use ninja_gen::cargo::RustOutput;
use ninja_gen::glob;
use ninja_gen::inputs;
use ninja_gen::Build;

pub fn build_rust(build: &mut Build) -> Result<()> {
    build_proto_descriptors_and_interfaces(build)
}

fn build_proto_descriptors_and_interfaces(build: &mut Build) -> Result<()> {
    let outputs = vec![
        RustOutput::Data("descriptors.bin", "rslib/proto/descriptors.bin"),
        RustOutput::Data("ts", "ts/lib/generated/backend.ts"),
    ];
    build.add_action(
        "rslib:proto",
        CargoBuild {
            inputs: inputs![glob!["{proto,rslib/proto}/**"], ":protoc_binary",],
            outputs: &outputs,
            extra_args: "-p anki_proto",
            release_override: None,
        },
    )?;
    Ok(())
}

pub fn check_rust(build: &mut Build) -> Result<()> {
    let inputs = inputs![
        glob!("{rslib/**,build/**}"),
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain.toml",
    ];
    build.add_action(
        "check:format:rust",
        CargoFormat {
            inputs: inputs.clone(),
            check_only: true,
            working_dir: Some("cargo/format"),
        },
    )?;
    build.add_action(
        "format:rust",
        CargoFormat {
            inputs: inputs.clone(),
            check_only: false,
            working_dir: Some("cargo/format"),
        },
    )?;

    let inputs = inputs![
        inputs,
        // generated files the crates need; ensure re-run on changes
        ":rslib:proto",
        // when env vars change the build hash gets updated
        "$builddir/env",
        "$builddir/buildhash",
    ];

    build.add_action(
        "check:clippy",
        CargoClippy {
            inputs: inputs.clone(),
        },
    )?;
    build.add_action("check:rust_test", CargoTest { inputs })?;

    Ok(())
}
