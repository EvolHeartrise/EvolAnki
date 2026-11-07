// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod error;

use std::fs::File;
use std::fs::FileTimes;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Component;
use std::path::Path;

use camino::Utf8Path;
use snafu::ResultExt;
use tempfile::NamedTempFile;

pub use crate::error::FileIoError;
pub use crate::error::FileIoSnafu;
pub use crate::error::FileOp;

pub type Result<T, E = FileIoError> = std::result::Result<T, E>;

/// See [File::create].
pub fn create_file(path: impl AsRef<Path>) -> Result<File> {
    File::create(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Create,
    })
}

/// See [File::open].
pub fn open_file(path: impl AsRef<Path>) -> Result<File> {
    File::open(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Open,
    })
}

pub fn open_file_ext(path: impl AsRef<Path>, options: OpenOptions) -> Result<File> {
    options.open(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Open,
    })
}

/// See [std::fs::write].
pub fn write_file(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    std::fs::write(&path, contents).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Write,
    })
}

pub fn write_file_and_flush(
    path: impl AsRef<Path> + Clone,
    contents: impl AsRef<[u8]>,
) -> Result<()> {
    let mut file = create_file(path.clone())?;
    file.write_all(contents.as_ref()).context(FileIoSnafu {
        path: path.clone().as_ref(),
        op: FileOp::Write,
    })?;
    file.sync_all().context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Sync,
    })
}

/// See [File::set_times].
pub fn set_file_times(path: impl AsRef<Path>, times: FileTimes) -> Result<()> {
    #[cfg(not(windows))]
    let file = open_file(&path)?;

    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        open_file_ext(
            &path,
            OpenOptions::new()
                .write(true)
                // It's required to modify the time attributes of a directory in windows system.
                .custom_flags(0x02000000) // FILE_FLAG_BACKUP_SEMANTICS
                .to_owned(),
        )?
    };

    file.set_times(times).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::SetFileTimes,
    })
}

/// See [std::fs::create_dir].
pub fn create_dir(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Create,
    })
}

/// See [std::fs::create_dir_all].
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    std::fs::create_dir_all(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Create,
    })
}

/// See [std::fs::read].
pub fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    std::fs::read(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Read,
    })
}

/// See [std::fs::read_to_string].
pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    std::fs::read_to_string(&path).context(FileIoSnafu {
        path: path.as_ref(),
        op: FileOp::Read,
    })
}

pub fn new_tempfile() -> Result<NamedTempFile> {
    NamedTempFile::new().context(FileIoSnafu {
        path: std::env::temp_dir(),
        op: FileOp::Create,
    })
}

pub fn new_tempfile_in(dir: impl AsRef<Path>) -> Result<NamedTempFile> {
    NamedTempFile::new_in(&dir).context(FileIoSnafu {
        path: dir.as_ref(),
        op: FileOp::Create,
    })
}

pub fn new_tempfile_in_parent_of(file: &Path) -> Result<NamedTempFile> {
    let dir = file.parent().unwrap_or(file);
    NamedTempFile::new_in(dir).context(FileIoSnafu {
        path: dir,
        op: FileOp::Create,
    })
}

/// Atomically replace the target path with the provided temp file.
///
/// If `fsync` is true, file data is synced to disk prior to renaming, and the
/// folder is synced on UNIX platforms after renaming. This minimizes the
/// chances of corruption if there is a crash or power loss directly after the
/// op, but it can be considerably slower.
pub fn atomic_rename(file: NamedTempFile, target: &Path, fsync: bool) -> Result<()> {
    if fsync {
        file.as_file().sync_all().context(FileIoSnafu {
            path: file.path(),
            op: FileOp::Sync,
        })?;
    }
    file.persist(target)?;
    #[cfg(not(windows))]
    if fsync {
        let abs_path;
        let parent = match target.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => {
                abs_path = std::path::absolute(target).context(FileIoSnafu {
                    path: target,
                    op: FileOp::Absolute,
                })?;
                abs_path.parent().ok_or(FileIoError {
                    path: abs_path.to_path_buf(),
                    op: FileOp::Parent,
                    source: std::io::Error::other("path has no parent"),
                })?
            }
        };
        open_file(parent)?.sync_all().context(FileIoSnafu {
            path: parent,
            op: FileOp::Sync,
        })?;
    }
    Ok(())
}

/// Like [std::fs::read_dir], but only yielding files. [Err]s are not filtered.
pub fn read_dir_files(path: impl AsRef<Path>) -> Result<ReadDirFiles> {
    std::fs::read_dir(&path)
        .map(ReadDirFiles)
        .context(FileIoSnafu {
            path: path.as_ref(),
            op: FileOp::Read,
        })
}

/// True if name does not contain any path separators.
pub fn filename_is_safe(name: &str) -> bool {
    let mut components = Path::new(name).components();
    let first_element_normal = components
        .next()
        .map(|component| matches!(component, Component::Normal(_)))
        .unwrap_or_default();

    first_element_normal && components.next().is_none()
}

pub struct ReadDirFiles(std::fs::ReadDir);

impl Iterator for ReadDirFiles {
    type Item = std::io::Result<std::fs::DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.0.next();
        if let Some(Ok(entry)) = next.as_ref() {
            match entry.metadata().map(|metadata| metadata.is_file()) {
                Ok(true) => next,
                Ok(false) => self.next(),
                Err(error) => Some(Err(error)),
            }
        } else {
            next
        }
    }
}

/// True if changed.
pub fn write_file_if_changed(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<bool> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    let changed = {
        read_file(path)
            .map(|existing| existing != contents)
            .unwrap_or(true)
    };

    match std::env::var("CARGO_PKG_NAME") {
        Ok(pkg) if pkg == "anki_proto" => {
            // at comptime for the proto crate, register implicit output as input
            println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
        }
        _ => {}
    }

    if changed {
        write_file(path, contents)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn is_case_sensitive(dir: &Path) -> bool {
    let opt = OpenOptions::new().write(true).create_new(true).to_owned();
    for _ in 0..100 {
        let fname = format!("__case-test-{}", rand::random::<u128>());
        let fname_upper = fname.to_uppercase();
        if open_file_ext(dir.join(&fname), opt.to_owned()).is_ok() {
            let sensitive = open_file_ext(dir.join(&fname_upper), opt.to_owned()).is_ok();
            let _ = std::fs::remove_file(dir.join(fname));
            if sensitive {
                let _ = std::fs::remove_file(dir.join(fname_upper));
            }
            return sensitive;
        }
    }
    cfg!(unix) && !cfg!(target_os = "macos")
}

pub trait ToUtf8Path {
    fn utf8(&self) -> Result<&Utf8Path>;
}

impl ToUtf8Path for Path {
    fn utf8(&self) -> Result<&Utf8Path> {
        Utf8Path::from_path(self).ok_or_else(|| FileIoError {
            path: self.into(),
            op: FileOp::DecodeUtf8Filename,
            source: std::io::Error::from(std::io::ErrorKind::InvalidData),
        })
    }
}

#[cfg(test)]
mod test {
    use std::env;
    use std::path::PathBuf;

    use tempfile::tempdir;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn path_traversal() {
        assert!(filename_is_safe("foo"));

        assert!(!filename_is_safe(".."));
        assert!(!filename_is_safe("foo/bar"));
        assert!(!filename_is_safe("/foo"));
        assert!(!filename_is_safe("../foo"));

        if cfg!(windows) {
            assert!(!filename_is_safe("foo\\bar"));
            assert!(!filename_is_safe("c:\\foo"));
            assert!(!filename_is_safe("\\foo"));
        }
    }

    struct TempCurrentDirectory {
        old_cwd: PathBuf,

        // We must hold ontop the TempDir reference so the directory doesn't get deleted. We don't
        // actually need to read it, though.
        #[allow(dead_code)]
        tmp_cwd: TempDir,
    }

    impl TempCurrentDirectory {
        fn new() -> Self {
            let old_cwd = env::current_dir().unwrap();
            let tmp_cwd = tempdir().unwrap();
            env::set_current_dir(&tmp_cwd).unwrap();
            Self { old_cwd, tmp_cwd }
        }
    }

    impl Drop for TempCurrentDirectory {
        fn drop(&mut self) {
            env::set_current_dir(&self.old_cwd).unwrap()
        }
    }

    #[test]
    fn test_atomic_rename_target_relative_path_one_component() {
        let tmp_cwd = TempCurrentDirectory::new();

        let tempfile = new_tempfile().unwrap();
        let target = Path::new("new-file.txt");
        atomic_rename(tempfile, target, true).unwrap();
        assert!(target.exists());

        drop(tmp_cwd);
    }
}
