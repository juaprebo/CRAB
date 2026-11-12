use anyhow::Result;
use std::path::Path;

pub trait FileOperations {
    fn is_regular_file(&self, path: &Path) -> Result<bool>;
    fn move_without_overwrite(&self, source: &Path, target: &Path) -> Result<()>;
}
