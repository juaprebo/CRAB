//! One format registry shared by library scanning, --formats and desktop setup.
use std::path::Path;
use std::sync::OnceLock;

pub struct Format {
    pub id: &'static str,
    pub label: &'static str,
    pub external: bool,
    pub extensions: &'static str,
}

pub fn all() -> &'static [Format] {
    static FORMATS: OnceLock<Vec<Format>> = OnceLock::new();
    FORMATS.get_or_init(|| {
        include_str!("../../../scripts/audio-formats.tsv")
            .lines()
            .map(|line| {
                let columns: Vec<_> = line.split('\t').collect();
                Format {
                    id: columns[0],
                    label: columns[1],
                    external: columns[2] == "ffmpeg",
                    extensions: columns[3],
                }
            })
            .collect()
    })
}

pub fn supports(path: &Path, external: bool) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    all().iter().any(|format| {
        (!format.external || external)
            && format
                .extensions
                .split_whitespace()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_are_unique_and_external_types_require_the_backend() {
        let mut ids = std::collections::HashSet::new();
        for format in all() {
            assert!(ids.insert(format.id));
            assert!(!format.label.is_empty());
        }
        assert!(supports(Path::new("album/SONG.AIFF"), false));
        assert!(supports(Path::new("song.mp2"), false));
        assert!(!supports(Path::new("song.opus"), false));
        assert!(supports(Path::new("song.opus"), true));
        assert!(supports(Path::new("song.wma"), true));
        assert!(!supports(Path::new("image.png"), true));
    }
}
