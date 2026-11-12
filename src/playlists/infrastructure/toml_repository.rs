use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::playlists::domain::playlist::Playlist;
pub struct TomlPlaylistRepository;
impl crate::playlists::domain::playlist_repository::PlaylistRepository for TomlPlaylistRepository {
    fn load(&self) -> Vec<Playlist> {
        load()
    }
    fn save(&self, playlists: &[Playlist]) -> Result<()> {
        save(playlists)
    }
}

#[cfg(test)]
use crate::playlists::domain::playlist::{add_tracks, rename};

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

#[cfg(test)]
mod rename_tests {
    use super::*;
    fn lists() -> Vec<Playlist> {
        vec![
            Playlist {
                name: "First".into(),
                tracks: vec!["/music/a.wav".into()],
            },
            Playlist {
                name: "Second".into(),
                tracks: vec![],
            },
        ]
    }
    #[test]
    fn rename_keeps_tracks_and_round_trips_with_the_new_name() {
        let mut lists = lists();
        let tracks = lists[0].tracks.clone();
        assert_eq!(rename(&mut lists, 0, "  New name  ").unwrap(), "First");
        assert_eq!(lists[0].name, "New name");
        assert_eq!(lists[0].tracks, tracks);
        let text = toml::to_string(&PlaylistFile {
            playlist: lists.clone(),
        })
        .unwrap();
        assert_eq!(
            toml::from_str::<PlaylistFile>(&text).unwrap().playlist,
            lists
        );
        assert_eq!(rename(&mut lists, 0, "NEW NAME").unwrap(), "New name");
    }
    #[test]
    fn invalid_names_and_duplicates_leave_playlists_unchanged() {
        let mut lists = lists();
        let original = lists.clone();
        for (index, name) in [(0, " "), (0, "second"), (8, "New")] {
            assert!(rename(&mut lists, index, name).is_err());
            assert_eq!(lists, original);
        }
    }
}
