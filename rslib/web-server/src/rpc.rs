// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! `POST /_anki/<camelCaseMethod>`: the equivalent of the post handlers in
//! qt/aqt/mediasrv.py, minus Python and Qt.
//!
//! The request body is the protobuf-encoded input and the response body the
//! protobuf-encoded output, as ts/lib/generated/post.ts expects. Failures are a
//! plain-text message with an error status, which post.ts surfaces to the page.

use std::collections::HashMap;
use std::sync::LazyLock;

use anki::backend::Backend;
use anki_proto::backend::backend_error::Kind;
use anki_proto::backend::BackendError;
use anki_proto::backend::BackendInit;
use anki_proto::generic;
use anki_proto::scheduler::CongratsInfoResponse;
use anyhow::anyhow;
use axum::http::StatusCode;
use prost::Message;

include!(concat!(env!("OUT_DIR"), "/methods.rs"));

/// RPCs passed straight to the backend. Anything not listed here, in
/// [UNSUPPORTED] or in [run] is refused, which keeps out e.g. closeCollection,
/// the raw database proxy and the sync methods.
///
/// Methods that read or write arbitrary server-side paths or fetch URLs
/// (import*, getCsvMetadata, addMediaFromPath/Url, getImageForOcclusion,
/// getAbsoluteMediaPath) are exposed by mediasrv.py but deliberately absent: a
/// browser page has no business naming files on the server. The editor and
/// importers will need upload-based replacements.
pub const ALLOWED: &[&str] = &[
    // CollectionService
    "latestProgress",
    "setWantsAbort",
    "getCustomColours",
    "getUndoStatus",
    "undo",
    "redo",
    // DeckService
    "deckTree",
    "getDeck",
    "getDeckNames",
    "getCurrentDeck",
    "setCurrentDeck",
    // I18nService
    "i18nResources",
    // NotesService
    "getNote",
    "getFieldNames",
    // NotetypesService
    "getNotetype",
    "getNotetypeNames",
    "getChangeNotetypeInfo",
    // Applied by a Qt dialog in mediasrv.py; a page can just call the RPC.
    "changeNotetype",
    // StatsService
    "cardStats",
    "getReviewLogs",
    "graphs",
    "getGraphPreferences",
    "setGraphPreferences",
    // TagsService
    "completeTag",
    // SchedulerService: reviewing
    "getQueuedCards",
    "answerCard",
    "describeNextStates",
    "stateIsLeech",
    "buryOrSuspendCards",
    "unburyDeck",
    "studiedToday",
    "schedTimingToday",
    // SchedulerService: FSRS tools on the deck options page
    "computeFsrsParams",
    "computeOptimalRetention",
    "evaluateParamsLegacy",
    "getOptimalRetentionParameters",
    "simulateFsrsReview",
    "simulateFsrsWorkload",
    // DeckConfigService
    "getDeckConfigsForUpdate",
    "updateDeckConfigs",
    "getIgnoredBeforeCount",
    "getRetentionWorkload",
    // CardRenderingService
    "renderExistingCard",
    "extractAvTags",
    "extractClozeForTyping",
    "compareAnswer",
    "encodeIriPaths",
    "decodeIriPaths",
    "htmlToTextLine",
    // ConfigService (getConfigJson is in run())
    "setConfigJson",
    "getConfigBool",
    // CardsService
    "getCard",
    "setFlag",
];

/// FrontendService methods that mediasrv.py implements by driving Qt. They
/// have no meaning here yet, so they fail loudly rather than pretending to
/// have worked.
pub const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "getSchedulingStatesWithContext",
        "reviewer state lives in the Qt reviewer",
    ),
    (
        "setSchedulingStates",
        "reviewer state lives in the Qt reviewer",
    ),
    (
        "deckOptionsRequireClose",
        "there is no deck options dialog to close",
    ),
    ("importDone", "there is no import dialog"),
    ("importDialogRequireClose", "there is no import dialog"),
    ("searchInBrowser", "there is no card browser yet"),
    ("convertPastedImage", "it relies on Qt's image handling"),
    ("openFilePicker", "the server can't show a file dialog"),
    ("openMedia", "the server can't open files on your desktop"),
    ("showInMediaFolder", "the server can't open a file manager"),
    ("recordAudio", "the server can't record audio"),
    ("playFile", "the server can't play audio; the browser must"),
    ("closeAddCards", "there is no add cards window"),
    ("closeEditCurrent", "there is no edit window"),
    ("openLink", "the page should open links itself"),
    ("askUser", "the page should ask with its own UI"),
    (
        "showMessageBox",
        "the page should show messages with its own UI",
    ),
    ("openFieldsDialog", "there is no fields dialog"),
    ("openCardsDialog", "there is no card layout dialog"),
    ("getProfileConfigJson", "there are no profiles"),
    ("setProfileConfigJson", "there are no profiles"),
    ("getMetaJson", "there is no profile metadata"),
    ("setMetaJson", "there is no profile metadata"),
    (
        "readClipboard",
        "the server has no access to your clipboard",
    ),
    (
        "writeClipboard",
        "the server has no access to your clipboard",
    ),
    ("saveCustomColours", "it reads Qt's colour dialog"),
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
    kind: Option<Kind>,
}

impl RpcError {
    fn new(status: StatusCode, message: String) -> Self {
        Self {
            status,
            message,
            kind: None,
        }
    }
}

/// Create a backend with the given UI language.
pub fn new_backend(lang: &str) -> anyhow::Result<Backend> {
    let init = BackendInit {
        preferred_langs: vec![lang.to_string()],
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
            Ok(err) => RpcError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                kind: Kind::try_from(err.kind).ok(),
                message: err.message,
            },
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
    match method {
        // mediasrv.py: a missing key reads as JSON null, not an error
        "getConfigJson" => match call(backend, method, input) {
            Err(RpcError {
                kind: Some(Kind::NotFoundError),
                ..
            }) => Ok(generic::Json {
                json: b"null".to_vec(),
            }
            .encode_to_vec()),
            other => other,
        },
        // mediasrv.py also moves the Qt main window to the overview screen when
        // cards are due; that decision now belongs to the page. We clear
        // bridge_commands_supported as the buried/custom study links in the
        // page call Qt bridge commands, which don't exist here.
        "congratsInfo" => {
            let output = call(backend, method, input)?;
            let mut info = CongratsInfoResponse::decode(output.as_slice())
                .map_err(|err| RpcError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
            info.bridge_commands_supported = false;
            Ok(info.encode_to_vec())
        }
        // Tells Qt that the webview has loaded, which has nothing to wait for
        // here. Every page load sends it, so it must succeed.
        "deckOptionsReady" => Ok(vec![]),
        _ if ALLOWED.contains(&method) => call(backend, method, input),
        _ => {
            if let Some((_, reason)) = UNSUPPORTED.iter().find(|(name, _)| *name == method) {
                Err(RpcError::new(
                    StatusCode::NOT_IMPLEMENTED,
                    format!("{method} is not available in the web version: {reason}."),
                ))
            } else {
                Err(RpcError::new(
                    StatusCode::NOT_FOUND,
                    format!("{method} not found"),
                ))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn allowed_methods_exist_in_the_backend() {
        for &method in ALLOWED.iter().chain(&["getConfigJson", "congratsInfo"]) {
            assert!(INDEXES.contains_key(method), "unknown method {method}");
        }
    }

    /// Frontend methods have indexes but no dispatcher in the Rust backend,
    /// so they must be handled here rather than allow-listed.
    #[test]
    fn frontend_service_is_fully_accounted_for() {
        let frontend = [
            "getSchedulingStatesWithContext",
            "setSchedulingStates",
            "deckOptionsRequireClose",
            "deckOptionsReady",
            "importDone",
            "importDialogRequireClose",
            "searchInBrowser",
            "convertPastedImage",
            "openFilePicker",
            "openMedia",
            "showInMediaFolder",
            "recordAudio",
            "playFile",
            "closeAddCards",
            "closeEditCurrent",
            "openLink",
            "askUser",
            "showMessageBox",
            "openFieldsDialog",
            "openCardsDialog",
            "getProfileConfigJson",
            "setProfileConfigJson",
            "getMetaJson",
            "setMetaJson",
            "readClipboard",
            "writeClipboard",
            "saveCustomColours",
        ];
        for method in frontend {
            assert!(!ALLOWED.contains(&method), "{method}");
            assert!(
                method == "deckOptionsReady" || UNSUPPORTED.iter().any(|(m, _)| *m == method),
                "{method}"
            );
        }
        assert_eq!(UNSUPPORTED.len(), frontend.len() - 1);
    }

    #[test]
    fn dangerous_methods_are_not_allowed() {
        for method in [
            "closeCollection",
            "openCollection",
            "createBackup",
            "syncCollection",
            "fullUploadOrDownload",
            "syncLogin",
            "checkDatabase",
            "importAnkiPackage",
            "addMediaFromPath",
            "addMediaFromUrl",
            "getAbsoluteMediaPath",
            "exportAnkiPackage",
        ] {
            assert!(!ALLOWED.contains(&method), "{method}");
        }
    }
}
