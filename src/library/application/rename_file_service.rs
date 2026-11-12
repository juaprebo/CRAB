use crate::library::domain::{file_name::destination, file_operations::FileOperations};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

pub struct RenameFileService {
    files: Box<dyn FileOperations>,
}
impl RenameFileService {
    pub fn new(files: Box<dyn FileOperations>) -> Self {
        Self { files }
    }
    pub fn execute(
        &self,
        source: &Path,
        name: &str,
        persist: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<PathBuf> {
        let target = destination(source, name)?;
        ensure!(
            self.files.is_regular_file(source)?,
            "Only regular audio files can be renamed"
        );
        if source == target {
            return Ok(target);
        }
        self.files
            .move_without_overwrite(source, &target)
            .context("Cannot rename file")?;
        if let Err(error) = persist(&target) {
            self.files
                .move_without_overwrite(&target, source)
                .with_context(|| {
                    format!(
                        "Saving references failed ({error:#}); rollback failed; file remains at {}",
                        target.display()
                    )
                })?;
            return Err(error.context("Cannot save references; file name restored"));
        }
        Ok(target)
    }
}
