// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anki_io::atomic_rename;
use anki_io::filename_is_safe;
use anki_io::new_tempfile_in;
use prost::Message;
use sha1::Digest;
use sha1::Sha1;
use zip::read::ZipFile;
use zip::result::ZipError;
use zip::ZipArchive;
use zstd::stream::copy_decode;

use super::meta::MetaExt;
use super::MediaEntries;
use super::MediaEntry;
use super::Meta;
use crate::import_export::ImportError;
use crate::media::files::normalize_filename;
use crate::prelude::*;

/// Like [MediaEntry], but with a safe filename and set zip filename.
#[derive(Debug)]
pub(super) struct SafeMediaEntry {
    pub(super) name: String,
    pub(super) size: u32,
    pub(super) sha1: Option<Sha1Hash>,
    pub(super) index: usize,
}

impl SafeMediaEntry {
    pub(super) fn from_entry(enumerated: (usize, MediaEntry)) -> Result<Self> {
        let (index, entry) = enumerated;
        if let Ok(sha1) = entry.sha1.try_into() {
            if !matches!(safe_normalized_file_name(&entry.name)?, Cow::Owned(_)) {
                return Ok(Self {
                    name: entry.name,
                    size: entry.size,
                    sha1: Some(sha1),
                    index,
                });
            }
        }
        Err(AnkiError::ImportError {
            source: ImportError::Corrupt,
        })
    }

    pub(super) fn from_legacy(legacy_entry: (&str, String)) -> Result<Self> {
        let zip_filename: usize = legacy_entry.0.parse()?;
        let name = match safe_normalized_file_name(&legacy_entry.1)? {
            Cow::Owned(new_name) => new_name,
            Cow::Borrowed(_) => legacy_entry.1,
        };
        Ok(Self {
            name,
            size: 0,
            sha1: None,
            index: zip_filename,
        })
    }

    pub(super) fn file_path(&self, media_folder: &Path) -> PathBuf {
        media_folder.join(&self.name)
    }

    pub(super) fn fetch_file<'a>(
        &self,
        archive: &'a mut ZipArchive<File>,
    ) -> Result<ZipFile<'a, File>> {
        match archive.by_name(&self.index.to_string()) {
            Ok(file) => Ok(file),
            Err(err) => invalid_input!(err, "{} missing from archive", self.index),
        }
    }

    pub(super) fn has_checksum_equal_to(
        &self,
        get_checksum: &mut impl FnMut(&str) -> Result<Option<Sha1Hash>>,
    ) -> Result<bool> {
        get_checksum(&self.name)
            .map(|opt| opt.is_some_and(|sha1| sha1 == self.sha1.expect("sha1 not set")))
    }

    pub(super) fn has_size_equal_to(&self, other_path: &Path) -> bool {
        fs::metadata(other_path).is_ok_and(|metadata| metadata.len() == self.size as u64)
    }

    /// Copy the archived file to the target folder, setting its hash if
    /// necessary.
    pub(super) fn copy_and_ensure_sha1_set(
        &mut self,
        archive: &mut ZipArchive<File>,
        target_folder: &Path,
        compressed: bool,
    ) -> Result<()> {
        let mut file = self.fetch_file(archive)?;
        let mut tempfile = new_tempfile_in(target_folder)?;
        if compressed {
            copy_decode(&mut file, &mut tempfile)?
        } else {
            self.sha1 = Some(copy_hashing(&mut file, &mut tempfile)?);
        }
        atomic_rename(tempfile, &self.file_path(target_folder), false)?;

        Ok(())
    }
}

pub(super) fn extract_media_entries(
    meta: &Meta,
    archive: &mut ZipArchive<File>,
) -> Result<Vec<SafeMediaEntry>> {
    let media_list_data = get_media_list_data(archive, meta)?;
    if meta.media_list_is_hashmap() {
        let map: HashMap<&str, String> = serde_json::from_slice(&media_list_data)?;
        map.into_iter().map(SafeMediaEntry::from_legacy).collect()
    } else {
        decode_safe_entries(&media_list_data)
    }
}

pub(super) fn safe_normalized_file_name(name: &str) -> Result<Cow<'_, str>> {
    if !filename_is_safe(name) {
        Err(AnkiError::ImportError {
            source: ImportError::Corrupt,
        })
    } else {
        Ok(normalize_filename(name))
    }
}

fn get_media_list_data(archive: &mut ZipArchive<File>, meta: &Meta) -> Result<Vec<u8>> {
    let mut file = match archive.by_name("media") {
        Ok(file) => file,
        Err(ZipError::FileNotFound) => {
            // Older AnkiDroid versions wrote colpkg files without a media map
            return Ok(b"{}".to_vec());
        }
        err => err?,
    };
    let mut buf = Vec::new();
    if meta.zstd_compressed() {
        copy_decode(file, &mut buf)?;
    } else {
        io::copy(&mut file, &mut buf)?;
    }
    Ok(buf)
}

pub(super) fn decode_safe_entries(buf: &[u8]) -> Result<Vec<SafeMediaEntry>> {
    let entries: MediaEntries = Message::decode(buf)?;
    entries
        .entries
        .into_iter()
        .enumerate()
        .map(SafeMediaEntry::from_entry)
        .collect()
}

/// Copies the data, returning its SHA1 hash.
fn copy_hashing(reader: &mut impl Read, writer: &mut impl Write) -> Result<Sha1Hash> {
    let mut hasher = Sha1::new();
    let mut buf = [0; 64 * 1024];
    loop {
        let count = match reader.read(&mut buf) {
            Ok(0) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        hasher.update(&buf[..count]);
        writer.write_all(&buf[..count])?;
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn normalization() {
        // legacy entries get normalized on deserialisation
        let entry = SafeMediaEntry::from_legacy(("1", "con".to_owned())).unwrap();
        assert_eq!(entry.name, "con_");

        // new-style entries should have been normalized on export
        let mut entries = Vec::new();
        MediaEntries {
            entries: vec![MediaEntry {
                name: "con".into(),
                ..Default::default()
            }],
        }
        .encode(&mut entries)
        .unwrap();
        assert!(decode_safe_entries(&entries).is_err());
    }
}
