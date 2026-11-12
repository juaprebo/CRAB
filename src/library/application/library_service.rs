use crate::library::domain::library::{Library, LocalTrack, SortKey};
use crate::library::domain::track_repository::{Scan, TrackRepository};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{Receiver, channel},
};

pub struct LibraryService {
    library: Library,
    repository: Arc<dyn TrackRepository>,
}

impl LibraryService {
    pub fn new(library: Library, repository: Arc<dyn TrackRepository>) -> Self {
        Self {
            library,
            repository,
        }
    }
    pub fn tracks(&self) -> &[LocalTrack] {
        self.library.tracks()
    }
    pub fn replace_tracks(&mut self, tracks: Vec<LocalTrack>) {
        self.library.replace_tracks(tracks);
    }
    pub fn favorites(&self) -> &BTreeSet<PathBuf> {
        self.library.favorites()
    }
    #[cfg(test)]
    pub fn set_favorite(&mut self, path: PathBuf, enabled: bool) {
        self.library.set_favorite(path, enabled);
    }
    pub fn toggle_favorites(&mut self, paths: &[PathBuf]) -> bool {
        self.library.toggle_favorites(paths)
    }
    pub fn sort(&mut self, key: SortKey) {
        self.library.sort(key);
    }
    pub fn visible_tracks(&self, filter: &str, favorites_only: bool) -> Vec<LocalTrack> {
        self.library.visible_tracks(filter, favorites_only)
    }
    pub fn rename_track(&mut self, source: &Path, renamed: &LocalTrack) {
        self.library.rename_track(source, renamed);
    }
    pub fn read_track(&self, path: PathBuf) -> LocalTrack {
        self.repository.read_track(path)
    }

    pub fn scan(&self, roots: Vec<PathBuf>, extra: Vec<PathBuf>, sort: SortKey) -> Receiver<Scan> {
        let repository = self.repository.clone();
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(repository.scan(roots, extra, sort));
        });
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Repository;
    impl TrackRepository for Repository {
        fn scan(&self, roots: Vec<PathBuf>, extra: Vec<PathBuf>, _: SortKey) -> Scan {
            Scan {
                tracks: roots.into_iter().map(crate::testutil::track).collect(),
                extra: extra.into_iter().map(crate::testutil::track).collect(),
            }
        }
        fn read_track(&self, path: PathBuf) -> LocalTrack {
            crate::testutil::track(path)
        }
    }
    #[test]
    fn scans_through_the_injected_repository_without_opening_files() {
        let path = PathBuf::from("/nonexistent/Music.mp3");
        let mut service = LibraryService::new(Library::new([path.clone()]), Arc::new(Repository));
        let scan = service
            .scan(vec![path.clone()], vec!["/other.mp3".into()], SortKey::Path)
            .recv()
            .unwrap();
        assert_eq!(scan.extra.len(), 1);
        service.replace_tracks(scan.tracks);
        assert_eq!(service.visible_tracks("music", true)[0].path, path);
        assert_eq!(service.read_track(path.clone()).path, path);
    }
}
