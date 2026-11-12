use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "linux")]
fn move_file(source: &Path, target: &Path) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn move_file(source: &Path, target: &Path) -> Result<()> {
    std::fs::hard_link(source, target)?;
    if let Err(error) = std::fs::remove_file(source) {
        let _ = std::fs::remove_file(target);
        return Err(error.into());
    }
    Ok(())
}

pub struct FileSystemFileOperations;
impl crate::library::domain::file_operations::FileOperations for FileSystemFileOperations {
    fn is_regular_file(&self, path: &Path) -> Result<bool> {
        Ok(std::fs::symlink_metadata(path)?.file_type().is_file())
    }
    fn move_without_overwrite(&self, source: &Path, target: &Path) -> Result<()> {
        move_file(source, target)
    }
}

#[cfg(test)]
mod tests {
    use crate::library::domain::file_name::destination;
    use anyhow::bail;
    fn rename(
        source: &std::path::Path,
        name: &str,
        persist: impl FnOnce(&std::path::Path) -> anyhow::Result<()>,
    ) -> anyhow::Result<std::path::PathBuf> {
        crate::library::application::rename_file_service::RenameFileService::new(Box::new(
            super::FileSystemFileOperations,
        ))
        .execute(source, name, persist)
    }

    use super::*;

    #[test]
    fn rejects_paths_and_extension_changes() {
        let source = Path::new("/music/old.mp3");
        for name in [
            "", " ", ".", "..", "../x.mp3", "/x.mp3", "x/y.mp3", "x\\y.mp3", "x.flac", "x\0.mp3",
        ] {
            assert!(destination(source, name).is_err(), "{name:?}");
        }
        assert_eq!(
            destination(source, " New name.mp3 ").unwrap(),
            Path::new("/music/New name.mp3")
        );
    }

    #[test]
    fn renames_without_overwriting_and_rolls_back_failed_persistence() {
        let dir = crate::testutil::temp_dir("rename");
        let source = dir.join("old.mp3");
        let target = dir.join("new.mp3");
        std::fs::write(&source, b"audio").unwrap();
        std::fs::write(&target, b"existing").unwrap();
        assert!(rename(&source, "new.mp3", |_| panic!("must not persist")).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
        std::fs::remove_file(&target).unwrap();
        assert!(rename(&source, "new.mp3", |_| bail!("disk full")).is_err());
        assert!(source.exists());
        assert!(!target.exists());
        let result = rename(&source, "new.mp3", |path| {
            assert!(path.exists());
            Ok(())
        })
        .unwrap();
        assert_eq!(result, target);
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"audio");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
