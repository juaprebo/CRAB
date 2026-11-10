use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use walkdir::WalkDir;

const EXTENSIONS: [&str; 8] = ["mp3", "flac", "ogg", "oga", "wav", "m4a", "aac", "mp4"];

#[derive(Clone, Debug)]
pub struct LocalTrack {
    pub path: PathBuf,
    pub title: String,
    pub folder: String,
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn to_track(path: PathBuf) -> LocalTrack {
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
    }
}

pub fn scan(roots: Vec<PathBuf>) -> Receiver<Vec<LocalTrack>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut tracks = Vec::new();
        for root in roots {
            if root.is_file() {
                if is_audio(&root) {
                    tracks.push(to_track(root));
                }
                continue;
            }
            for entry in WalkDir::new(&root)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() && is_audio(entry.path()) {
                    tracks.push(to_track(entry.into_path()));
                }
            }
        }
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        let _ = tx.send(tracks);
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
        let tracks = scan(vec![root.clone()]).recv().unwrap();
        assert_eq!(tracks.len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
