// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! `POST /_anki/<camelCaseMethod>`: the equivalent of the post handlers in
//! upstream Anki's qt/aqt/mediasrv.py.
//!
//! The request body is the protobuf-encoded input and the response body the
//! protobuf-encoded output, as ts/lib/generated/post.ts expects. Failures are a
//! plain-text message with an error status, which post.ts surfaces to the page.

use std::collections::HashMap;
use std::sync::LazyLock;

use anki::backend::Backend;
use anki_proto::backend::BackendError;
use anki_proto::backend::BackendInit;
use anyhow::anyhow;
use axum::http::StatusCode;
use prost::Message;

include!(concat!(env!("OUT_DIR"), "/methods.rs"));

/// The RPCs that a browser can reach. This server has no login, so it's what
/// the pages use and nothing else, which `allow_list_matches_the_pages` in
/// tests/http.rs enforces. An admin tool would link the backend rather than use
/// these.
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
    // I18nService
    "i18nResources",
    // NotesService
    "getNote",
    // NotetypesService
    "getNotetype",
    // StatsService
    "cardStats",
    "graphs",
    "getGraphPreferences",
    "setGraphPreferences",
    // SchedulerService: reviewing
    "getQueuedCards",
    "answerCard",
    "describeNextStates",
    "stateIsLeech",
    "congratsInfo",
    "studiedToday",
    // SchedulerService: FSRS tools on the deck options page
    "computeFsrsParams",
    "computeOptimalRetention",
    "evaluateParamsLegacy",
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
    // CardsService
    "getCard",
];

static INDEXES: LazyLock<HashMap<&'static str, (u32, u32)>> = LazyLock::new(|| {
    ALL_METHODS
        .iter()
        .map(|&(name, service, method)| (name, (service, method)))
        .collect()
});

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
    let init = BackendInit {
        preferred_langs: vec!["en".to_string()],
        locale_folder_path: String::new(),
        server: false,
    };
    anki::backend::init_backend(&init.encode_to_vec()).map_err(|e| anyhow!(e))
}

/// Call any backend method by its camelCase name, bypassing the allow-list.
/// Blocks; collection access is serialised by the mutex inside the backend.
pub fn call(backend: &Backend, method: &str, input: &[u8]) -> Result<Vec<u8>, RpcError> {
    let Some(&(service_idx, method_idx)) = INDEXES.get(method) else {
        return Err(RpcError::new(
            StatusCode::NOT_FOUND,
            format!("{method} not found"),
        ));
    };
    backend
        .run_service_method(service_idx, method_idx, input)
        .map_err(|bytes| match BackendError::decode(bytes.as_slice()) {
            Ok(err) => RpcError::new(StatusCode::INTERNAL_SERVER_ERROR, err.message),
            Err(err) => RpcError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("undecodable backend error: {err}"),
            ),
        })
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
            assert!(INDEXES.contains_key(method), "unknown method {method}");
        }
    }

    #[test]
    fn dangerous_methods_are_not_allowed() {
        for method in [
            "closeCollection",
            "openCollection",
            "createBackup",
            "checkDatabase",
            "importAnkiPackage",
            "addMediaFromPath",
            "getAbsoluteMediaPath",
            "exportAnkiPackage",
            "buryOrSuspendCards",
            "unburyDeck",
            "setFlag",
            "changeNotetype",
            "getConfigJson",
            "setConfigJson",
        ] {
            assert!(!ALLOWED.contains(&method), "{method}");
        }
    }
}
