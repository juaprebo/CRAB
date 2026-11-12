use super::scanner::{read_tracks, to_track};
use crate::library::domain::library::LocalTrack;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    mtime: u64,
    size: u64,
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    track: u32,
    disc: u32,
    year: u32,
    gain_db: Option<f32>,
    duration_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
struct CacheFile {
    version: u32,
    ffmpeg: bool,
    entries: Vec<Entry>,
}

pub fn default_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    dirs::cache_dir().map(|d| d.join("crab").join("library.json"))
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((modified.as_nanos() as u64, meta.len()))
}

fn entry_of(track: &LocalTrack, mtime: u64, size: u64) -> Entry {
    Entry {
        path: track.path.clone(),
        mtime,
        size,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album_artist: track.album_artist.clone(),
        album: track.album.clone(),
        track: track.track,
        disc: track.disc,
        year: track.year,
        gain_db: track.gain_db,
        duration_ms: track.duration.map(|d| d.as_millis() as u64),
    }
}

fn track_of(entry: Entry) -> LocalTrack {
    let mut track = to_track(entry.path);
    track.title = entry.title;
    track.artist = entry.artist;
    track.album_artist = entry.album_artist;
    track.album = entry.album;
    track.track = entry.track;
    track.disc = entry.disc;
    track.year = entry.year;
    track.gain_db = entry.gain_db;
    track.duration = entry.duration_ms.map(Duration::from_millis);
    track
}

fn load(file: &Path, ffmpeg: bool) -> HashMap<PathBuf, Entry> {
    let parsed = std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CacheFile>(&bytes).ok());
    match parsed {
        Some(cache) if cache.version == VERSION && cache.ffmpeg == ffmpeg => cache
            .entries
            .into_iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect(),
        _ => HashMap::new(),
    }
}

fn save(file: &Path, ffmpeg: bool, entries: Vec<Entry>) {
    let cache = CacheFile {
        version: VERSION,
        ffmpeg,
        entries,
    };
    let Ok(bytes) = serde_json::to_vec(&cache) else {
        return;
    };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temp = file.with_extension("json.tmp");
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::rename(&temp, file);
    }
}

pub fn read_cached(paths: Vec<PathBuf>, cache: Option<&Path>) -> Vec<LocalTrack> {
    let Some(file) = cache else {
        return read_tracks(paths);
    };
    let ffmpeg = crate::playback::infrastructure::ffmpeg::available();
    let mut known = load(file, ffmpeg);
    let mut slots: Vec<Option<LocalTrack>> = Vec::with_capacity(paths.len());
    let mut stamps = Vec::with_capacity(paths.len());
    let mut misses = Vec::new();
    for path in &paths {
        let current = stamp(path);
        match (current, known.remove(path)) {
            (Some((mtime, size)), Some(entry)) if entry.mtime == mtime && entry.size == size => {
                slots.push(Some(track_of(entry)));
            }
            _ => {
                slots.push(None);
                misses.push(path.clone());
            }
        }
        stamps.push(current);
    }
    let changed = !misses.is_empty();
    let mut fresh = read_tracks(misses).into_iter();
    let tracks: Vec<LocalTrack> = slots
        .into_iter()
        .zip(&paths)
        .map(|(slot, path)| {
            slot.or_else(|| fresh.next())
                .unwrap_or_else(|| to_track(path.clone()))
        })
        .collect();
    let before = known.len();
    known.retain(|path, _| stamp(path).is_some());
    if !changed && known.len() == before {
        return tracks;
    }
    let mut entries: Vec<Entry> = known.into_values().collect();
    entries.extend(
        tracks
            .iter()
            .zip(&stamps)
            .filter_map(|(track, stamp)| stamp.map(|(mtime, size)| entry_of(track, mtime, size))),
    );
    save(file, ffmpeg, entries);
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::config::WriteOptions;
    use lofty::tag::{Accessor, Tag, TagExt, TagType};

    fn tagged(path: &Path, title: &str) {
        crate::testutil::write_wav(path, 1);
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(title.to_string());
        tag.save_to_path(path, WriteOptions::default()).unwrap();
    }

    #[test]
    fn reuses_unchanged_entries_and_rereads_changed_files() {
        let dir = crate::testutil::temp_dir("cache");
        let song = dir.join("a.wav");
        let other = dir.join("b.wav");
        tagged(&song, "Original");
        tagged(&other, "Other");
        let file = dir.join("library.json");

        let first = read_cached(vec![song.clone(), other.clone()], Some(&file));
        assert_eq!(first[0].title, "Original");
        assert_eq!(first[1].title, "Other");
        assert!(first[0].duration.is_some());

        let mut cache: CacheFile = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        for entry in &mut cache.entries {
            if entry.path == song {
                entry.title = "From cache".into();
            }
        }
        std::fs::write(&file, serde_json::to_vec(&cache).unwrap()).unwrap();
        let second = read_cached(vec![song.clone(), other.clone()], Some(&file));
        assert_eq!(second[0].title, "From cache");
        assert_eq!(second[1].title, "Other");

        tagged(&song, "Changed on disk, longer title");
        let third = read_cached(vec![song.clone(), other.clone()], Some(&file));
        assert_eq!(third[0].title, "Changed on disk, longer title");
        assert_eq!(third[1].title, "Other");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn drops_deleted_files_and_ignores_corrupt_caches() {
        let dir = crate::testutil::temp_dir("cache-drop");
        let song = dir.join("a.wav");
        tagged(&song, "Song");
        let file = dir.join("library.json");
        read_cached(vec![song.clone()], Some(&file));
        std::fs::remove_file(&song).unwrap();
        let other = dir.join("b.wav");
        tagged(&other, "Other");
        read_cached(vec![other.clone()], Some(&file));
        let cache: CacheFile = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert!(cache.entries.iter().all(|e| e.path != song));
        std::fs::write(&file, b"not json").unwrap();
        let tracks = read_cached(vec![other.clone()], Some(&file));
        assert_eq!(tracks[0].title, "Other");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
