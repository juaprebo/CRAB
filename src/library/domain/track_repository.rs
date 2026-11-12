use super::library::{LocalTrack, SortKey};
use std::path::PathBuf;

pub struct Scan {
    pub tracks: Vec<LocalTrack>,
    pub extra: Vec<LocalTrack>,
}

pub trait TrackRepository: Send + Sync {
    fn scan(&self, roots: Vec<PathBuf>, extra: Vec<PathBuf>, sort: SortKey) -> Scan;
    fn read_track(&self, path: PathBuf) -> LocalTrack;
}
