// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Managing a collection from `evolanki-admin`, which is how cards get changed
//! at all in this app, as it has no editor. Commands take and return JSON, and
//! run the same in the server, over its Unix socket, as in the CLI, when no
//! server is running. docs/admin-cli.md describes them for whoever is using it.

pub mod commands;
pub mod convert;
pub mod socket;
pub mod validate;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anki::backend::Backend;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;

use crate::backups::Backups;

/// More ids than this in one command gets a backup first.
const BULK_IDS: usize = 20;

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub command: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Changes nothing that matters. Takes no backup.
    Read,
    /// Takes a backup first if it covers a search, or many ids.
    Write,
    /// Always takes a backup first.
    Destructive,
}

pub struct Command {
    pub name: &'static str,
    pub effect: Effect,
    /// The JSON it takes, for `evolanki-admin commands`.
    pub args: &'static str,
    pub help: &'static str,
    pub run: fn(&Admin, &Value) -> Result<Value>,
}

pub struct Admin {
    pub backend: Backend,
    pub backups: Arc<Backups>,
    collection: PathBuf,
    media_dir: PathBuf,
}

impl Admin {
    pub fn new(
        backend: Backend,
        backups: Arc<Backups>,
        collection: &Path,
        media_dir: PathBuf,
    ) -> Self {
        Self {
            backend,
            backups,
            collection: collection.to_path_buf(),
            media_dir,
        }
    }

    pub fn collection(&self) -> &Path {
        &self.collection
    }

    pub fn media_dir(&self) -> &Path {
        &self.media_dir
    }

    /// Run a command, and turn its outcome into the response to print. If a
    /// backup that's due can't be taken, the command doesn't run.
    pub fn execute(&self, request: &Request) -> Value {
        let Some(command) = commands::find(&request.command) else {
            return json!({
                "ok": false,
                "error": format!(
                    "unknown command {:?}; `evolanki-admin commands` lists them",
                    request.command
                ),
            });
        };
        let mut backup = None;
        if needs_backup(command.effect, &request.args) {
            match self.backups.try_take(&self.backend) {
                Ok(path) => backup = Some(path.display().to_string()),
                Err(err) => {
                    return json!({
                        "ok": false,
                        "error": format!("not running this, as the backup failed: {err:#}"),
                    })
                }
            }
        }
        match (command.run)(self, &request.args) {
            Ok(result) => {
                let mut response = json!({"ok": true, "result": result});
                if let Some(backup) = backup {
                    response["backup"] = backup.into();
                }
                response
            }
            Err(err) => {
                let mut response = json!({"ok": false, "error": format!("{err:#}")});
                if let Some(backup) = backup {
                    response["backup"] = backup.into();
                }
                response
            }
        }
    }
}

fn needs_backup(effect: Effect, args: &Value) -> bool {
    match effect {
        Effect::Read => false,
        Effect::Destructive => true,
        Effect::Write => {
            let many = |key: &str| {
                args.get(key)
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.len() > BULK_IDS)
            };
            args.get("search").is_some() || many("ids") || many("note_ids")
        }
    }
}
