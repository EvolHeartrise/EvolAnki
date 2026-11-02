// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Opening and closing a collection through the backend.

use std::path::Path;
use std::path::PathBuf;

use anki::backend::Backend;
use anki_proto::collection::CloseCollectionRequest;
use anki_proto::collection::OpenCollectionRequest;
use anki_proto::generic;
use anyhow::Context;
use anyhow::Result;

use crate::rpc::call_typed;

/// Returns the media folder. Opening a collection migrates it to FSRS.
pub fn open_collection(backend: &Backend, path: &Path) -> Result<PathBuf> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| parent.display().to_string())?;
    let path = parent.canonicalize()?.join(
        path.file_name()
            .context("collection path has no filename")?,
    );

    // same layout as media_paths_from_col_path() in upstream Anki, so that
    // collections are interchangeable with a desktop profile
    let (media_dir, media_db) = media_paths(&path);
    std::fs::create_dir_all(&media_dir)?;

    let _: generic::Empty = call_typed(
        backend,
        "openCollection",
        &OpenCollectionRequest {
            collection_path: utf8(&path)?,
            media_folder_path: utf8(&media_dir)?,
            media_db_path: utf8(&media_db)?,
        },
    )?;
    Ok(media_dir)
}

/// The media folder and its database, for a collection at this path.
pub fn media_paths(collection: &Path) -> (PathBuf, PathBuf) {
    (
        collection.with_extension("media"),
        collection.with_extension("media.db2"),
    )
}

pub fn utf8(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}

/// Waits for any running operation to release the collection.
pub fn close_collection(backend: &Backend) -> Result<()> {
    let _: generic::Empty = call_typed(backend, "closeCollection", &CloseCollectionRequest {})?;
    Ok(())
}
