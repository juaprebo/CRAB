use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LocalTrack {
    pub path: PathBuf,
    pub title: String,
    pub folder: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub track: u32,
    pub disc: u32,
    pub year: u32,
    pub gain_db: Option<f32>,
    pub duration: Option<std::time::Duration>,
}

impl LocalTrack {
    pub fn detail(&self) -> String {
        match (self.artist.is_empty(), self.album.is_empty()) {
            (false, false) => format!("{} - {}", self.artist, self.album),
            (false, true) => self.artist.clone(),
            (true, false) => self.album.clone(),
            (true, true) => self.folder.clone(),
        }
    }

    pub fn artist_or_folder(&self) -> String {
        if self.artist.is_empty() {
            self.folder.clone()
        } else {
            self.artist.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    #[default]
    Path,
    Title,
    Artist,
    Album,
}

impl SortKey {
    pub fn from_name(name: &str) -> Option<SortKey> {
        [
            SortKey::Path,
            SortKey::Title,
            SortKey::Artist,
            SortKey::Album,
        ]
        .into_iter()
        .find(|s| s.name().eq_ignore_ascii_case(name))
    }

    pub fn name(self) -> &'static str {
        match self {
            SortKey::Path => "path",
            SortKey::Title => "title",
            SortKey::Artist => "artist",
            SortKey::Album => "album",
        }
    }

    pub fn next(self) -> SortKey {
        match self {
            SortKey::Path => SortKey::Title,
            SortKey::Title => SortKey::Artist,
            SortKey::Artist => SortKey::Album,
            SortKey::Album => SortKey::Path,
        }
    }
}

pub fn sort_tracks(tracks: &mut [LocalTrack], key: SortKey) {
    let lower = |s: &str| s.to_lowercase();
    match key {
        SortKey::Path => tracks.sort_by(|a, b| a.path.cmp(&b.path)),
        SortKey::Title => {
            tracks.sort_by_cached_key(|t| (lower(&t.title), t.path.clone()));
        }
        SortKey::Artist => {
            tracks.sort_by_cached_key(|t| {
                (
                    lower(&t.artist_or_folder()),
                    lower(&t.album),
                    t.disc,
                    t.track,
                    lower(&t.title),
                )
            });
        }
        SortKey::Album => {
            tracks.sort_by_cached_key(|t| {
                (
                    lower(&t.album),
                    lower(&t.artist_or_folder()),
                    t.disc,
                    t.track,
                    lower(&t.title),
                )
            });
        }
    }
}

#[derive(Default)]
pub struct Library {
    tracks: Vec<LocalTrack>,
    favorites: std::collections::BTreeSet<PathBuf>,
}

impl Library {
    pub fn new(favorites: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            tracks: Vec::new(),
            favorites: favorites.into_iter().collect(),
        }
    }

    pub fn tracks(&self) -> &[LocalTrack] {
        &self.tracks
    }

    pub fn replace_tracks(&mut self, tracks: Vec<LocalTrack>) {
        self.tracks = tracks;
    }

    pub fn favorites(&self) -> &std::collections::BTreeSet<PathBuf> {
        &self.favorites
    }

    pub fn set_favorite(&mut self, path: PathBuf, enabled: bool) {
        if enabled {
            self.favorites.insert(path);
        } else {
            self.favorites.remove(&path);
        }
    }

    pub fn toggle_favorites(&mut self, paths: &[PathBuf]) -> bool {
        let remove = !paths.is_empty() && paths.iter().all(|path| self.favorites.contains(path));
        for path in paths {
            self.set_favorite(path.clone(), !remove);
        }
        remove
    }

    pub fn sort(&mut self, key: SortKey) {
        sort_tracks(&mut self.tracks, key);
    }

    pub fn visible_tracks(&self, filter: &str, favorites_only: bool) -> Vec<LocalTrack> {
        let words: Vec<_> = filter.split_whitespace().map(str::to_lowercase).collect();
        self.tracks
            .iter()
            .filter(|track| {
                matches_filter(track, &words)
                    && (!favorites_only || self.favorites.contains(&track.path))
            })
            .cloned()
            .collect()
    }

    pub fn rename_track(&mut self, source: &std::path::Path, renamed: &LocalTrack) {
        for track in &mut self.tracks {
            if track.path == source {
                *track = renamed.clone();
            }
        }
        if self.favorites.remove(source) {
            self.favorites.insert(renamed.path.clone());
        }
    }
}

pub fn matches_filter(track: &LocalTrack, words: &[String]) -> bool {
    let haystack = format!(
        "{} {} {} {} {}",
        track.title, track.artist, track.album_artist, track.album, track.folder
    )
    .to_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str, artist: &str) -> LocalTrack {
        LocalTrack {
            path: format!("/music/{name}.mp3").into(),
            title: name.into(),
            folder: "music".into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: String::new(),
            track: 0,
            disc: 0,
            year: 0,
            gain_db: None,
            duration: None,
        }
    }

    #[test]
    fn rescanning_and_renaming_preserve_favorites_and_filtering() {
        let original = track("One", "Björk");
        let other = track("Two", "Other");
        let mut library = Library::new([original.path.clone()]);
        library.replace_tracks(vec![original.clone(), other.clone()]);
        assert_eq!(library.visible_tracks("BJÖRK one", true).len(), 1);
        let mut renamed = original.clone();
        renamed.path = "/music/new.mp3".into();
        library.rename_track(&original.path, &renamed);
        assert!(!library.favorites().contains(&original.path));
        assert!(library.favorites().contains(&renamed.path));
        library.replace_tracks(vec![other, renamed.clone()]);
        assert_eq!(library.visible_tracks("one", true)[0].path, renamed.path);
        assert!(library.visible_tracks("missing", false).is_empty());
    }

    #[test]
    fn favorite_groups_are_added_together_then_removed_together() {
        let first = PathBuf::from("/music/first.mp3");
        let second = PathBuf::from("/music/second.mp3");
        let mut library = Library::new([first.clone()]);
        assert!(!library.toggle_favorites(&[first.clone(), second.clone()]));
        assert_eq!(library.favorites().len(), 2);
        assert!(library.toggle_favorites(&[first, second]));
        assert!(library.favorites().is_empty());
    }
}
