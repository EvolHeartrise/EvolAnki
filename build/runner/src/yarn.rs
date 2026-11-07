// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::env;
use std::path::Path;
use std::process::Command;

use clap::Args;

use crate::run::run_command;

#[derive(Args)]
pub struct YarnArgs {
    yarn_bin: String,
    stamp: String,
}

pub fn setup_yarn(args: YarnArgs) {
    link_node_modules();

    if env::var("OFFLINE_BUILD").is_ok() {
        println!("OFFLINE_BUILD is set");
        println!("Running yarn with '--offline' and '--ignore-scripts'.");
        run_command(
            Command::new(&args.yarn_bin)
                .arg("install")
                .arg("--offline")
                .arg("--ignore-scripts"),
        );
    } else {
        run_command(
            Command::new(&args.yarn_bin)
                .arg("install")
                .arg("--immutable"),
        );
    }

    std::fs::write(args.stamp, b"").unwrap();
}

/// Unfortunately a lot of the node ecosystem expects the output folder to
/// reside in the repo root, so we need to link in our output folder.
fn link_node_modules() {
    let target = Path::new("node_modules");
    if target.exists() {
        if !target.is_symlink() {
            panic!("please remove the node_modules folder from the repo root");
        }
    } else {
        std::os::unix::fs::symlink("out/node_modules", target).unwrap();
    }
}
