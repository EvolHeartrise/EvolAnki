// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! `POST /_anki/<camelCaseMethod>`: the pages' calls into the backend.
//!
//! The request body is the protobuf-encoded input and the response body the
//! protobuf-encoded output, as ts/lib/generated/post.ts expects. Failures are a
//! plain-text message with an error status, which post.ts surfaces to the page.

use anki::backend::Backend;
use anki::prelude::I18n;
use anyhow::anyhow;
use axum::http::StatusCode;
use prost::Message;

/// The RPCs that a browser can reach. This server has no login, so it's what
/// the pages use and nothing else, which `allow_list_matches_the_pages` in
/// tests/http.rs enforces. `evolanki-admin` doesn't go through here: it calls
/// [call] from its own Unix socket, which isn't reachable over HTTP.
pub const ALLOWED: &[&str] = &[
    // CollectionService
    "latestProgress",
    "setWantsAbort",
    "getUndoStatus",
    "undo",
    // DeckService
    "deckTree",
    "getDeck",
    "getCurrentDeck",
    "setCurrentDeck",
    "setDeckCollapsed",
    // NotesService
    "getNote",
    // NotetypesService
    "getNotetype",
    // StatsService
    "graphs",
    "getGraphPreferences",
    "setGraphPreferences",
    // SchedulerService: reviewing
    "getQueuedCards",
    "answerCard",
    "studiedToday",
    // SchedulerService: FSRS tools on the deck options page
    "computeFsrsParams",
    "computeOptimalRetention",
    "simulateFsrsReview",
    "simulateFsrsWorkload",
    // DeckConfigService
    "getDeckConfig",
    "getDeckConfigsForUpdate",
    "updateDeckConfigs",
    "getIgnoredBeforeCount",
    "getRetentionWorkload",
    // CardRenderingService
    "renderExistingCard",
    "extractAvTags",
    "renderMarkdown",
    "extractClozeForTyping",
    "compareAnswer",
    "encodeIriPaths",
    // ConfigService
    "getConfigBool",
];

pub struct RpcError {
    pub status: StatusCode,
    pub message: String,
}

impl RpcError {
    fn new(status: StatusCode, message: String) -> Self {
        Self { status, message }
    }
}

pub fn new_backend() -> anyhow::Result<Backend> {
    Ok(Backend::new(I18n::new()))
}

/// Call any backend method by its camelCase name, bypassing the allow-list.
/// Blocks; collection access is serialised by the mutex inside the backend.
pub fn call(backend: &Backend, method: &str, input: &[u8]) -> Result<Vec<u8>, RpcError> {
    match backend.run_method(method, input) {
        None => Err(RpcError::new(
            StatusCode::NOT_FOUND,
            format!("{method} not found"),
        )),
        Some(result) => result.map_err(|err| {
            RpcError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                err.message(backend.i18n()),
            )
        }),
    }
}

/// Typed version of [call], for the server's own use.
pub fn call_typed<O: Message + Default>(
    backend: &Backend,
    method: &str,
    input: &impl Message,
) -> anyhow::Result<O> {
    let output = call(backend, method, &input.encode_to_vec())
        .map_err(|err| anyhow!("{method}: {}", err.message))?;
    Ok(O::decode(output.as_slice())?)
}

/// Handle a request from a page. Blocking.
pub fn run(backend: &Backend, method: &str, input: &[u8]) -> Result<Vec<u8>, RpcError> {
    if ALLOWED.contains(&method) {
        call(backend, method, input)
    } else {
        Err(RpcError::new(
            StatusCode::NOT_FOUND,
            format!("{method} not found"),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn allowed_methods_exist_in_the_backend() {
        for &method in ALLOWED {
            assert!(
                Backend::METHODS.contains(&method),
                "unknown method {method}"
            );
        }
    }

    #[test]
    fn dangerous_methods_are_not_allowed() {
        for method in [
            "closeCollection",
            "openCollection",
            "checkDatabase",
            "checkMedia",
            "importAnkiPackage",
            "importCollectionPackage",
            "removeNotes",
            "removeCards",
            "removeDecks",
            "suspendCards",
        ] {
            // so the list can't go stale as methods are removed
            assert!(
                Backend::METHODS.contains(&method),
                "unknown method {method}"
            );
            assert!(!ALLOWED.contains(&method), "{method}");
        }
    }
}
