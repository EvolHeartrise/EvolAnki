// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Writes packages in the current format, as Anki exports them, for import
//! tests. EvolAnki itself doesn't export.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anki_io::new_tempfile;
use anki_io::read_dir_files;
use anki_io::read_file;
use prost::Message;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use super::meta::MetaExt;
use super::MediaEntries;
use super::MediaEntry;
use super::Meta;
use crate::collection::CollectionBuilder;
use crate::import_export::gather::ExchangeData;
use crate::media::files::sha1_of_data;
use crate::prelude::*;

impl Collection {
    /// An .apkg of the notes matching `search`, with their cards, decks,
    /// notetypes, deck settings and review history, and, if `with_media`,
    /// every file in the media folder.
    pub(crate) fn export_apkg(
        &mut self,
        out_path: impl AsRef<Path>,
        search: impl TryIntoSearch,
        with_media: bool,
    ) -> Result<()> {
        let mut data = ExchangeData::default();
        data.gather_data(self, search, true, true)?;
        let temp_col = new_tempfile()?;
        let mut col = CollectionBuilder::new(temp_col.path()).build()?;
        col.storage.db.execute_batch("DELETE FROM notetypes")?;
        col.insert_data(&data)?;
        col.storage
            .set_creation_stamp(self.storage.creation_stamp()?)?;
        col.set_creation_utc_offset(self.get_creation_utc_offset())?;
        col.close()?;
        let media = if with_media {
            media_files(&self.media_folder)?
        } else {
            vec![]
        };
        write_package(out_path.as_ref(), temp_col.path(), &media)
    }

    /// A .colpkg of the whole collection and its media.
    pub(crate) fn export_colpkg(self, out_path: impl AsRef<Path>) -> Result<()> {
        let col_path = self.col_path.clone();
        let media = media_files(&self.media_folder)?;
        self.close()?;
        write_package(out_path.as_ref(), &col_path, &media)
    }
}

fn media_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vec![];
    for entry in read_dir_files(folder)? {
        files.push(entry?.path());
    }
    Ok(files)
}

fn write_package(out_path: &Path, col_path: &Path, media: &[PathBuf]) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(out_path)?);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("meta", stored)?;
    zip.write_all(&Meta::new().encode_to_vec())?;
    zip.start_file("collection.anki21b", stored)?;
    zip.write_all(&zstd::encode_all(read_file(col_path)?.as_slice(), 0)?)?;
    let mut entries = vec![];
    for (index, path) in media.iter().enumerate() {
        let data = read_file(path)?;
        zip.start_file(index.to_string(), stored)?;
        zip.write_all(&zstd::encode_all(data.as_slice(), 0)?)?;
        entries.push(MediaEntry {
            name: path.file_name().unwrap().to_str().unwrap().into(),
            size: data.len() as u32,
            sha1: sha1_of_data(&data).to_vec(),
            legacy_zip_filename: None,
        });
    }
    zip.start_file("media", stored)?;
    let media_map = MediaEntries { entries }.encode_to_vec();
    zip.write_all(&zstd::encode_all(media_map.as_slice(), 0)?)?;
    zip.finish()?;
    Ok(())
}
