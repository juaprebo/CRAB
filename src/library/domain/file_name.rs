use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum FileNameError {
    InvalidName,
    ChangedExtension,
}

impl std::fmt::Display for FileNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "Enter a file name without a directory",
            Self::ChangedExtension => "Keep the original file extension",
        })
    }
}

impl std::error::Error for FileNameError {}

pub fn destination(source: &Path, name: &str) -> Result<PathBuf, FileNameError> {
    let name = name.trim();
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || name.contains(['/', '\\', '\0'])
    {
        return Err(FileNameError::InvalidName);
    }
    let target = source.with_file_name(name);
    if source.extension() != target.extension() {
        return Err(FileNameError::ChangedExtension);
    }
    Ok(target)
}
