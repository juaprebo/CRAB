use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaylistError {
    EmptyName,
    MissingPlaylist,
    DuplicateName(String),
}

impl std::fmt::Display for PlaylistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => write!(f, "Playlist name cannot be empty"),
            Self::MissingPlaylist => write!(f, "Playlist no longer exists"),
            Self::DuplicateName(name) => write!(f, "A playlist named {name} already exists"),
        }
    }
}

impl std::error::Error for PlaylistError {}

pub fn find(playlists: &[Playlist], name: &str) -> Option<usize> {
    playlists
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

pub fn rename(
    playlists: &mut [Playlist],
    index: usize,
    name: &str,
) -> Result<String, PlaylistError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PlaylistError::EmptyName);
    }
    if index >= playlists.len() {
        return Err(PlaylistError::MissingPlaylist);
    }
    if playlists
        .iter()
        .enumerate()
        .any(|(i, list)| i != index && list.name.eq_ignore_ascii_case(name))
    {
        return Err(PlaylistError::DuplicateName(name.into()));
    }
    Ok(std::mem::replace(
        &mut playlists[index].name,
        name.to_string(),
    ))
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
