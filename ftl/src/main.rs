// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod garbage_collection;
mod serialize;

use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use garbage_collection::garbage_collect_ftl_entries;
use garbage_collection::write_ftl_json;
use garbage_collection::GarbageCollectArgs;
use garbage_collection::WriteJsonArgs;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract references from all Rust, Python, TS, Svelte and Designer files
    /// in the given roots, convert them to ftl names case and write them as
    /// a json to the target file.
    WriteJson(WriteJsonArgs),
    /// Delete every entry in the ftl files that is not mentioned in another
    /// message or a given json.
    GarbageCollect(GarbageCollectArgs),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::WriteJson(args) => write_ftl_json(args),
        Command::GarbageCollect(args) => garbage_collect_ftl_entries(args),
    }
}
