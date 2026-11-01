// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Copy a desktop Anki profile to a new place, for serving. The original is
//! only ever read: its database through a read-only URI, its other files as
//! plain reads, and it's hashed before and after to show nothing changed. A
//! snapshot of the source is kept beside the copy, for `validate` to compare
//! against, so that it never needs the original again.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anki::storage::copy_database;
use anki::storage::verify_copied_collection;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use percent_encoding::utf8_percent_encode;
use percent_encoding::AsciiSet;
use percent_encoding::NON_ALPHANUMERIC;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

use super::commands::fill_missing_memory_states;
use crate::collection::close_collection;
use crate::collection::media_paths;
use crate::collection::open_collection;
use crate::rpc;

const URI_PATH_SAFE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.');

/// `<target folder>/conversion`: the snapshot, and what was hashed.
pub fn conversion_folder(target: &Path) -> PathBuf {
    target
        .parent()
        .unwrap_or(Path::new("."))
        .join("conversion")
}

pub fn snapshot_path(target: &Path) -> PathBuf {
    conversion_folder(target).join("source-snapshot.anki2")
}

pub fn manifest_path(target: &Path) -> PathBuf {
    conversion_folder(target).join("manifest.json")
}

/// A closed collection, opened so that SQLite can't write to it or its -wal and
/// -shm files, or even take a lock. It would not see what's in a non-empty WAL,
/// so check that first.
pub fn open_immutable(path: &Path) -> Result<Connection> {
    let path = path.canonicalize()?;
    let path = path.to_str().context("path isn't UTF-8")?;
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        utf8_percent_encode(path, URI_PATH_SAFE)
    );
    let db = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    db.create_collation("unicase", |a, b| {
        a.to_lowercase().cmp(&b.to_lowercase())
    })?;
    Ok(db)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| path.display().to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// A digest of a folder's file names and contents, and how many files it has.
pub fn hash_folder(folder: &Path) -> Result<Value> {
    let mut names = BTreeMap::new();
    let mut size = 0;
    for entry in fs::read_dir(folder).with_context(|| folder.display().to_string())? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            size += entry.metadata()?.len();
            let name = entry.file_name().to_string_lossy().into_owned();
            names.insert(name, sha256_file(&entry.path())?);
        }
    }
    let mut hasher = Sha256::new();
    for (name, hash) in &names {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    Ok(json!({
        "files": names.len(),
        "bytes": size,
        "sha256": hex::encode(hasher.finalize()),
    }))
}

/// Everything in the source profile that conversion could conceivably touch.
pub fn hash_source(collection: &Path, media: &Path, media_db: &Path) -> Result<Value> {
    let mut out = BTreeMap::new();
    for (key, path) in [
        ("collection", collection.to_path_buf()),
        ("wal", PathBuf::from(format!("{}-wal", collection.display()))),
        ("shm", PathBuf::from(format!("{}-shm", collection.display()))),
        ("media_db", media_db.to_path_buf()),
    ] {
        if path.exists() {
            let metadata = fs::metadata(&path)?;
            out.insert(
                key,
                json!({
                    "sha256": sha256_file(&path)?,
                    "bytes": metadata.len(),
                    "mtime": metadata.modified()?
                        .duration_since(std::time::UNIX_EPOCH)?.as_secs(),
                }),
            );
        }
    }
    out.insert("media_folder", hash_folder(media)?);
    Ok(json!(out))
}

fn command_line(pid: &str) -> Vec<String> {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|bytes| {
            bytes
                .split(|b| *b == 0)
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn looks_like_desktop_anki(args: &[String]) -> bool {
    let Some(program) = args.first() else {
        return false;
    };
    let base = |arg: &str| {
        Path::new(arg)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    let base_program = base(program);
    base_program == "anki"
        || base_program == "aqt"
        || (base_program.starts_with("python")
            && args
                .iter()
                .skip(1)
                .any(|arg| arg == "aqt" || base(arg) == "anki" || base(arg) == "aqt"))
}

/// Refuses if desktop Anki is running, or anything has the profile's files open.
fn check_nothing_is_using(profile: &Path) -> Result<()> {
    let own_pid = std::process::id().to_string();
    let profile = profile.canonicalize()?;
    for entry in fs::read_dir("/proc")? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !name.bytes().all(|b| b.is_ascii_digit()) || name == own_pid {
            continue;
        }
        let args = command_line(&name);
        if looks_like_desktop_anki(&args) {
            bail!(
                "desktop Anki seems to be running (process {name}: {}). Close it first.",
                args.join(" ")
            );
        }
        // other users' processes can't be read, and have no business here
        let Ok(files) = fs::read_dir(format!("/proc/{name}/fd")) else {
            continue;
        };
        for file in files.flatten() {
            if fs::read_link(file.path()).is_ok_and(|open| open.starts_with(&profile)) {
                bail!(
                    "process {name} ({}) has a file open in {}. Close it first.",
                    args.join(" "),
                    profile.display()
                );
            }
        }
    }
    Ok(())
}

/// Whether two database files are the same apart from their 100-byte headers,
/// which the backup API and the change of journal mode legitimately touch.
fn same_after_header(a: &Path, b: &Path) -> Result<bool> {
    let (a, b) = (fs::read(a)?, fs::read(b)?);
    Ok(a.len() == b.len() && a.len() >= 100 && a[100..] == b[100..])
}

fn make_private(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| path.display().to_string())
}

pub fn convert(source: &Path, target: &Path) -> Result<Value> {
    let source = source
        .canonicalize()
        .with_context(|| source.display().to_string())?;
    let profile = source.parent().context("source has no folder")?;
    let (source_media, source_media_db) = media_paths(&source);
    if !source_media.is_dir() {
        bail!("no media folder at {}", source_media.display());
    }

    let target_folder = target.parent().filter(|p| !p.as_os_str().is_empty());
    let target_folder = target_folder.unwrap_or(Path::new("."));
    fs::create_dir_all(target_folder)?;
    make_private(target_folder, 0o700)?;
    let target_folder = target_folder.canonicalize()?;
    let target = target_folder.join(target.file_name().context("target has no file name")?);
    if target_folder.starts_with(profile) || target == source {
        bail!("the target must be outside the source profile");
    }
    let (target_media, _) = media_paths(&target);
    let snapshot = snapshot_path(&target);
    for existing in [&target, &target_media, &snapshot] {
        if existing.exists() {
            bail!(
                "{} exists; this doesn't overwrite anything, so choose another \
                 folder or move it away",
                existing.display()
            );
        }
    }

    check_nothing_is_using(profile)?;
    let wal = PathBuf::from(format!("{}-wal", source.display()));
    if fs::metadata(&wal).map(|m| m.len()).unwrap_or(0) > 0 {
        bail!(
            "{} isn't empty, so the last close wasn't clean. Open and close the \
             profile in desktop Anki first.",
            wal.display()
        );
    }

    let hashes_before = hash_source(&source, &source_media, &source_media_db)?;

    // the snapshot stays as it is, and the target starts as a copy of it
    let folder = conversion_folder(&target);
    fs::create_dir_all(&folder)?;
    make_private(&folder, 0o700)?;
    {
        let db = open_immutable(&source)?;
        copy_database(&db, &snapshot)?;
    }
    verify_copied_collection(&snapshot)?;
    make_private(&snapshot, 0o400)?;
    let identical_pages = same_after_header(&source, &snapshot)?;
    if !identical_pages {
        bail!("the snapshot differs from the source");
    }
    if source_media_db.exists() {
        fs::copy(&source_media_db, folder.join("source-media.db2"))?;
    }
    fs::copy(&snapshot, &target)?;
    make_private(&target, 0o600)?;

    let copied = Command::new("cp")
        .args(["-a", "--"])
        .arg(&source_media)
        .arg(&target_media)
        .status()
        .context("couldn't run cp")?;
    if !copied.success() {
        bail!("cp of the media folder failed");
    }
    let media = hash_folder(&target_media)?;
    if media != hashes_before["media_folder"] {
        bail!("the copied media differs from the source");
    }

    let backend = rpc::new_backend()?;
    open_collection(&backend, &target)?;
    let memory_states_added = fill_missing_memory_states(&backend)?;
    close_collection(&backend)?;

    let hashes_after = hash_source(&source, &source_media, &source_media_db)?;
    let unchanged = hashes_before == hashes_after;
    let manifest = json!({
        "source": source,
        "target": target,
        "snapshot": snapshot,
        "snapshot_pages_identical_to_source": identical_pages,
        "source_hashes_before": hashes_before,
        "source_hashes_after": hashes_after,
        "source_unchanged": unchanged,
        "memory_states_added": memory_states_added,
        "converted_at": chrono::Local::now().to_rfc3339(),
    });
    fs::write(
        manifest_path(&target),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    if !unchanged {
        bail!(
            "THE SOURCE CHANGED during conversion; see {}",
            manifest_path(&target).display()
        );
    }
    Ok(manifest)
}
