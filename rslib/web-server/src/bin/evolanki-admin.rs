// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Manage a collection from the command line, with JSON in and out. See
//! docs/admin-cli.md.

use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use evolanki_server::admin::commands;
use evolanki_server::admin::convert;
use evolanki_server::admin::socket;
use evolanki_server::admin::socket::Sent;
use evolanki_server::admin::validate;
use evolanki_server::admin::Admin;
use evolanki_server::admin::Effect;
use evolanki_server::admin::Request;
use evolanki_server::backups::opening_would_change;
use evolanki_server::backups::Backups;
use evolanki_server::collection::close_collection;
use evolanki_server::collection::open_collection;
use evolanki_server::rpc;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use serde_json::json;
use serde_json::Value;

#[derive(Parser)]
#[command(about = "Manage an EvolAnki collection, with JSON in and out")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List every command, with the JSON it takes.
    Commands,
    /// Run a command on a collection. It goes through the running server if
    /// there is one, and otherwise opens the collection directly.
    Run {
        /// The collection (.anki2). Or set EVOLANKI_COLLECTION.
        #[arg(long)]
        collection: Option<PathBuf>,
        /// A name from `commands`, such as note.add.
        command: String,
        /// JSON arguments. `-` reads them from stdin; leave out for none.
        args: Option<String>,
    },
    /// Copy a desktop Anki profile's collection and media to a new place,
    /// without changing the original, and migrate the copy.
    Convert {
        /// The desktop profile's collection.anki2.
        #[arg(long)]
        source: PathBuf,
        /// Where the copy goes. Mustn't exist.
        #[arg(long)]
        target: PathBuf,
    },
    /// Compare a converted collection against the snapshot `convert` left, and
    /// write a report. Works on throwaway copies, so changes nothing.
    Validate {
        /// The converted collection.
        #[arg(long)]
        target: PathBuf,
        /// Where to write the report.
        #[arg(long)]
        report: PathBuf,
        /// Folder for the throwaway copies, deleted afterwards.
        #[arg(long)]
        work_dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let _ = anki::log::set_global_logger(None);
    let printed = match Cli::parse().action {
        Action::Commands => Ok(list_commands()),
        Action::Run {
            collection,
            command,
            args,
        } => run(collection, command, args),
        Action::Convert { source, target } => {
            convert::convert(&source, &target).map(|result| json!({"ok": true, "result": result}))
        }
        Action::Validate {
            target,
            report,
            work_dir,
        } => validate::validate(&target, &report, &work_dir)
            .map(|result| json!({"ok": true, "result": result})),
    };
    let response = printed.unwrap_or_else(|err| json!({"ok": false, "error": format!("{err:#}")}));
    // a reader that went away, such as head, is no reason to panic
    let _ = writeln!(std::io::stdout(), "{response}");
    if response["ok"] == true {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn list_commands() -> Value {
    let commands: Vec<_> = commands::COMMANDS
        .iter()
        .map(|command| {
            json!({
                "name": command.name,
                "args": command.args,
                "help": command.help,
                "backup": match command.effect {
                    Effect::Read => "no",
                    Effect::Write => "if bulk",
                    Effect::Destructive => "always",
                },
            })
        })
        .collect();
    json!({"ok": true, "result": commands})
}

fn run(collection: Option<PathBuf>, command: String, args: Option<String>) -> Result<Value> {
    let collection = collection
        .or_else(|| std::env::var_os("EVOLANKI_COLLECTION").map(PathBuf::from))
        .context("say which collection, with --collection or EVOLANKI_COLLECTION")?;
    let text = match args.as_deref() {
        None => "{}".to_string(),
        Some("-") => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            text
        }
        Some(text) => text.to_string(),
    };
    let mut args: Value = serde_json::from_str(&text).context("the arguments aren't JSON")?;
    if !args.is_object() {
        bail!("the arguments must be a JSON object");
    }
    commands::absolute_paths(&mut args)?;
    let request = Request { command, args };

    let mut response = match socket::send(&collection, &request)? {
        Sent::Response(response) => {
            let mut response = response;
            response["via"] = "server".into();
            response
        }
        Sent::NoServer => {
            let mut response = run_directly(&collection, &request)?;
            response["via"] = "direct".into();
            response
        }
    };
    response["command"] = request.command.into();
    Ok(response)
}

/// A mistyped path shouldn't make a new, empty collection.
fn run_directly(collection: &Path, request: &Request) -> Result<Value> {
    if !collection.is_file() {
        bail!("{} isn't a file", collection.display());
    }
    if commands::find(&request.command).is_none() {
        bail!(
            "unknown command {:?}; `evolanki-admin commands` lists them",
            request.command
        );
    }
    let backups = Arc::new(Backups::new(collection)?);
    // opening an SM-2 collection migrates it, as does one with an older schema or
    // settings shared between decks, which is worth having a copy from before,
    // as the server does
    if opening_would_change(collection) {
        backups.take_before_opening(collection)?;
    } else if !fsrs_is_on(collection) {
        backups.take_of_closed_file(collection);
    }
    let backend = rpc::new_backend()?;
    let media_dir = open_collection(&backend, collection).with_context(|| {
        format!(
            "couldn't open {}, so it's probably in use by something other than \
             evolanki-server, such as desktop Anki",
            collection.display()
        )
    })?;
    let admin = Admin::new(backend.clone(), backups, collection, media_dir);
    let response = admin.execute(request);
    close_collection(&backend)?;
    Ok(response)
}

fn fsrs_is_on(collection: &Path) -> bool {
    let read = || -> rusqlite::Result<Vec<u8>> {
        let db = Connection::open_with_flags(collection, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.query_row("select val from config where key = 'fsrs'", [], |row| {
            row.get(0)
        })
    };
    read().is_ok_and(|value| value == b"true")
}
