use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use walkdir::WalkDir;

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

pub fn is_audio(path: &Path) -> bool {
    crate::formats::supports(path, false)
        || (crate::formats::supports(path, true) && crate::ffmpeg::available())
}

pub fn to_track(path: PathBuf) -> LocalTrack {
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let folder = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    LocalTrack {
        path,
        title,
        folder,
        artist: String::new(),
        album_artist: String::new(),
        album: String::new(),
        track: 0,
        disc: 0,
        year: 0,
        gain_db: None,
        duration: None,
    }
}

fn parse_gain(text: &str) -> Option<f32> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        .collect();
    cleaned.parse::<f32>().ok().filter(|g| g.is_finite())
}

fn apply_tag(track: &mut LocalTrack, tag: &Tag) {
    track.album_artist = tag
        .get_string(ItemKey::AlbumArtist)
        .unwrap_or_default()
        .trim()
        .to_string();
    if let Some(title) = tag.title().filter(|t| !t.trim().is_empty()) {
        track.title = title.trim().to_string();
    }
    if let Some(artist) = tag.artist().filter(|t| !t.trim().is_empty()) {
        track.artist = artist.trim().to_string();
    }
    if let Some(album) = tag.album().filter(|t| !t.trim().is_empty()) {
        track.album = album.trim().to_string();
    }
    track.track = tag.track().unwrap_or(0);
    track.disc = tag.disk().unwrap_or(0);
    track.year = tag.date().map(|d| d.year as u32).unwrap_or(0);
    track.gain_db = tag
        .get_string(ItemKey::ReplayGainTrackGain)
        .and_then(parse_gain);
}

pub fn read_track(path: PathBuf) -> LocalTrack {
    let mut track = to_track(path);
    let probed = Probe::open(&track.path).and_then(|probe| {
        probe
            .options(ParseOptions::new().read_cover_art(false))
            .read()
    });
    if let Ok(file) = probed {
        let duration = file.properties().duration();
        if !duration.is_zero() {
            track.duration = Some(duration);
        }
        if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
            apply_tag(&mut track, tag);
        }
    } else {
        read_external_tags(&mut track);
    }
    track
}

fn read_external_tags(track: &mut LocalTrack) {
    if !crate::ffmpeg::available() {
        return;
    }
    let Some(metadata) = crate::ffmpeg::probe(&track.path) else {
        return;
    };
    track.duration = crate::ffmpeg::duration(&metadata);
    let Some(tags) = metadata
        .get("format")
        .and_then(|format| format.get("tags"))
        .and_then(|tags| tags.as_object())
    else {
        return;
    };
    let value = |key: &str| {
        tags.iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .and_then(|(_, value)| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if let Some(title) = value("title") {
        track.title = title.into();
    }
    if let Some(artist) = value("artist") {
        track.artist = artist.into();
    }
    if let Some(album) = value("album") {
        track.album = album.into();
    }
    if let Some(artist) = value("album_artist") {
        track.album_artist = artist.into();
    }
    let number = |key| {
        value(key)
            .and_then(|text| text.split('/').next())
            .and_then(|text| text.parse().ok())
            .unwrap_or(0)
    };
    track.track = number("track");
    track.disc = number("disc");
}

pub fn read_cover(path: &Path) -> Option<Vec<u8>> {
    let from_tag = Probe::open(path)
        .ok()
        .and_then(|probe| probe.read().ok())
        .and_then(|tagged| {
            tagged
                .primary_tag()
                .into_iter()
                .chain(tagged.tags().iter())
                .find_map(|tag| {
                    tag.pictures()
                        .first()
                        .map(|picture| picture.data().to_vec())
                })
        });
    if from_tag.is_some() {
        return from_tag;
    }
    let dir = path.parent()?;
    for name in ["cover", "folder", "front", "album", "albumart"] {
        for ext in ["jpg", "jpeg", "png"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if let Ok(bytes) = std::fs::read(&candidate) {
                return Some(bytes);
            }
        }
    }
    None
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

pub fn read_tracks(paths: Vec<PathBuf>) -> Vec<LocalTrack> {
    if paths.len() < 64 {
        return paths.into_iter().map(read_track).collect();
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 8);
    let chunk = paths.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .cloned()
                        .map(read_track)
                        .collect::<Vec<LocalTrack>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

pub fn collect(path: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if path.is_file() {
        if is_audio(path) {
            found.push(path.to_path_buf());
        }
        return found;
    }
    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && is_audio(entry.path()) {
            found.push(entry.into_path());
        }
    }
    found.sort();
    found
}

pub struct Scan {
    pub tracks: Vec<LocalTrack>,
    pub extra: Vec<LocalTrack>,
}

pub fn scan(roots: Vec<PathBuf>, extra: Vec<PathBuf>, sort: SortKey) -> Receiver<Scan> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut paths = Vec::new();
        for root in roots {
            if root.is_file() {
                if is_audio(&root) {
                    paths.push(root);
                }
                continue;
            }
            for entry in WalkDir::new(&root)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() && is_audio(entry.path()) {
                    paths.push(entry.into_path());
                }
            }
        }
        paths.sort();
        paths.dedup();
        let extra: Vec<PathBuf> = extra
            .into_iter()
            .filter(|p| paths.binary_search(p).is_err())
            .collect();
        let mut tracks = read_tracks(paths);
        sort_tracks(&mut tracks, sort);
        let _ = tx.send(Scan {
            tracks,
            extra: read_tracks(extra),
        });
    });
    rx
}

pub fn watch(roots: &[PathBuf]) -> Option<(RecommendedWatcher, Receiver<()>)> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if let Ok(event) = event
            && !matches!(event.kind, EventKind::Access(_))
        {
            let _ = tx.send(());
        }
    })
    .ok()?;
    let mut watching = false;
    for root in roots {
        if root.exists() && watcher.watch(root, RecursiveMode::Recursive).is_ok() {
            watching = true;
        }
    }
    if watching { Some((watcher, rx)) } else { None }
}

fn roots_file() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::cache_dir)
        .map(|d| d.join("crab").join("library_roots.txt"))
}

pub fn load_saved_roots() -> Vec<PathBuf> {
    let Some(path) = roots_file() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(|l| PathBuf::from(l.trim()))
                .filter(|p| !p.as_os_str().is_empty() && p.exists())
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_roots(roots: &[PathBuf]) {
    let Some(path) = roots_file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    let _ = std::fs::write(path, text.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tagged(dir: &Path, name: &str, title: &str, artist: &str, album: &str, n: u32) -> PathBuf {
        use lofty::config::WriteOptions;
        use lofty::tag::{TagExt, TagType};
        let path = dir.join(name);
        crate::testutil::write_wav(&path, 1);
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(title.to_string());
        tag.set_artist(artist.to_string());
        tag.set_album(album.to_string());
        tag.insert_text(ItemKey::AlbumArtist, "Album Artist".into());
        tag.set_track(n);
        tag.insert_text(ItemKey::ReplayGainTrackGain, "-6.50 dB".to_string());
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        path
    }

    #[test]
    fn reads_tags_and_sorts() {
        let dir = std::env::temp_dir().join(format!("crab-tags-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = tagged(&dir, "a.wav", "Zebra", "Beta Band", "Second", 1);
        let b = tagged(&dir, "b.wav", "Apple", "Alpha", "First", 2);
        let c = tagged(&dir, "c.wav", "Mango", "Alpha", "First", 1);
        let plain = dir.join("plain.wav");
        crate::testutil::write_wav(&plain, 1);

        let track = read_track(a.clone());
        assert_eq!(track.title, "Zebra");
        assert_eq!(track.artist, "Beta Band");
        assert_eq!(track.album_artist, "Album Artist");
        assert_eq!(track.album, "Second");
        assert_eq!(track.track, 1);
        assert_eq!(track.gain_db, Some(-6.5));
        assert!(track.duration.is_some());
        assert_eq!(track.detail(), "Beta Band - Second");

        let fallback = read_track(plain.clone());
        assert_eq!(fallback.title, "plain");
        assert_eq!(fallback.artist, "");
        assert_eq!(fallback.detail(), fallback.folder);

        let mut tracks = vec![a, b, c]
            .into_iter()
            .map(read_track)
            .collect::<Vec<_>>();
        sort_tracks(&mut tracks, SortKey::Title);
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Apple", "Mango", "Zebra"]);
        sort_tracks(&mut tracks, SortKey::Artist);
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Mango", "Apple", "Zebra"]);
        sort_tracks(&mut tracks, SortKey::Album);
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Mango", "Apple", "Zebra"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gain_parsing() {
        assert_eq!(parse_gain("-3.20 dB"), Some(-3.2));
        assert_eq!(parse_gain("+1.5dB"), Some(1.5));
        assert_eq!(parse_gain("loud"), None);
    }

    #[test]
    fn watcher_new_folder() {
        let root = std::env::temp_dir().join(format!("crab-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let (_watcher, rx) = watch(std::slice::from_ref(&root)).unwrap();
        let album = root.join("new album");
        std::fs::create_dir_all(&album).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        while rx.try_recv().is_ok() {}
        crate::testutil::write_wav(&album.join("song.wav"), 1);
        assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
        let scanned = scan(vec![root.clone()], Vec::new(), SortKey::Path)
            .recv()
            .unwrap();
        assert_eq!(scanned.tracks.len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
