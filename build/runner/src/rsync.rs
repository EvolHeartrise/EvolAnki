// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::process::Command;

use camino::Utf8Path;
use clap::Args;

use crate::run::run_command;

#[derive(Args)]
pub struct RsyncArgs {
    #[arg(long, value_delimiter(','), allow_hyphen_values(true))]
    extra_args: Vec<String>,
    #[arg(long)]
    prefix: String,
    #[arg(long, required(true), num_args(..))]
    inputs: Vec<String>,
    #[arg(long)]
    output_dir: String,
}

pub fn rsync_files(args: RsyncArgs) {
    let output_dir = Utf8Path::new(&args.output_dir).canonicalize_utf8().unwrap();
    run_command(
        Command::new("rsync")
            .current_dir(&args.prefix)
            .arg("--relative")
            .args(args.extra_args)
            .args(&args.inputs)
            .arg(output_dir),
    );
}
