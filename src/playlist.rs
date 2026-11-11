use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct PlaylistFile {
    #[serde(default)]
    playlist: Vec<Playlist>,
}

fn playlists_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("crab").join("playlists.toml"))
}

pub fn load() -> Vec<Playlist> {
    let Some(path) = playlists_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<PlaylistFile>(&text).ok())
        .map(|file| file.playlist)
        .unwrap_or_default()
}

pub fn save(playlists: &[Playlist]) -> Result<()> {
    let path = playlists_path().ok_or_else(|| anyhow!("no config directory available"))?;
    let file = PlaylistFile {
        playlist: playlists.to_vec(),
    };
    let text = toml::to_string_pretty(&file).context("cannot encode playlists")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("cannot create config directory")?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).context("cannot write playlists")?;
    std::fs::rename(&temp, &path).context("cannot replace playlists")?;
    Ok(())
}

pub fn find(playlists: &[Playlist], name: &str) -> Option<usize> {
    playlists
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

pub fn add_tracks(
    playlists: &mut Vec<Playlist>,
    name: &str,
    paths: Vec<PathBuf>,
) -> (usize, usize) {
    let index = match find(playlists, name) {
        Some(index) => index,
        None => {
            playlists.push(Playlist {
                name: name.trim().to_string(),
                tracks: Vec::new(),
            });
            playlists.len() - 1
        }
    };
    let list = &mut playlists[index];
    let mut added = 0;
    for path in paths {
        if !list.tracks.contains(&path) {
            list.tracks.push(path);
            added += 1;
        }
    }
    (index, added)
}

pub fn parse_m3u(text: &str, base: &std::path::Path) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| !line.contains("://"))
        .map(|line| {
            let path = PathBuf::from(line);
            if path.is_absolute() {
                path
            } else {
                base.join(path)
            }
        })
        .collect()
}

pub fn to_m3u(entries: &[(PathBuf, String, Option<u64>)]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for (path, label, seconds) in entries {
        let seconds = seconds.map_or(-1, |s| s as i64);
        out.push_str(&format!("#EXTINF:{seconds},{label}\n"));
        out.push_str(&path.display().to_string());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    #[test]
    fn creates_and_appends() {
        let mut lists = Vec::new();
        let (index, added) = add_tracks(&mut lists, " Gym ", vec![p("/a.mp3"), p("/b.mp3")]);
        assert_eq!((index, added), (0, 2));
        assert_eq!(lists[0].name, "Gym");
        let (index, added) = add_tracks(&mut lists, "gym", vec![p("/b.mp3"), p("/c.mp3")]);
        assert_eq!((index, added), (0, 1));
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].tracks, vec![p("/a.mp3"), p("/b.mp3"), p("/c.mp3")]);
        let (index, _) = add_tracks(&mut lists, "Chill", Vec::new());
        assert_eq!(index, 1);
    }

    #[test]
    fn round_trips_through_toml() {
        let lists = vec![
            Playlist {
                name: "One".into(),
                tracks: vec![p("/x/a.mp3"), p("/y/b.flac")],
            },
            Playlist {
                name: "Empty".into(),
                tracks: Vec::new(),
            },
        ];
        let text = toml::to_string_pretty(&PlaylistFile {
            playlist: lists.clone(),
        })
        .unwrap();
        let back: PlaylistFile = toml::from_str(&text).unwrap();
        assert_eq!(back.playlist, lists);
    }

    #[test]
    fn m3u_round_trip() {
        let entries = vec![
            (p("/music/a.mp3"), "Artist - A".to_string(), Some(200)),
            (p("/music/b.flac"), "B".to_string(), None),
        ];
        let text = to_m3u(&entries);
        assert!(text.starts_with("#EXTM3U\n#EXTINF:200,Artist - A\n/music/a.mp3\n"));
        let parsed = parse_m3u(&text, std::path::Path::new("/elsewhere"));
        assert_eq!(parsed, vec![p("/music/a.mp3"), p("/music/b.flac")]);
    }

    #[test]
    fn m3u_relative_and_urls() {
        let text = "#EXTM3U\r\n\r\nrel/song.mp3\r\nhttp://radio.example/stream\r\n#comment\r\n/abs/x.ogg\r\n";
        let parsed = parse_m3u(text, std::path::Path::new("/lists"));
        assert_eq!(parsed, vec![p("/lists/rel/song.mp3"), p("/abs/x.ogg")]);
    }
}
