use crate::library::domain::library::LocalTrack;
use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use ratatui::widgets::ListState;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Songs,
    Artists,
    Albums,
}

impl View {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "songs" => Some(Self::Songs),
            "artists" => Some(Self::Artists),
            "albums" => Some(Self::Albums),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AlbumKey {
    artist: String,
    album: String,
    // Untagged albums from different folders must not collapse into one.
    folder: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Location {
    Songs,
    Artists,
    Albums,
    Artist(String),
    Album(AlbumKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Artist(String),
    Album(AlbumKey),
    Song(PathBuf),
}

pub struct Row {
    target: Target,
    pub title: String,
    pub detail: String,
    pub tracks: Vec<LocalTrack>,
}

pub struct Playback {
    pub tracks: Vec<LocalTrack>,
    pub index: usize,
    pub name: String,
}

impl Row {
    pub fn display_title(&self, language: Language) -> String {
        match &self.target {
            Target::Artist(_) => self
                .tracks
                .first()
                .map(|track| display_artist(track, language))
                .unwrap_or_default(),
            Target::Album(_) => self
                .tracks
                .first()
                .map(|track| display_album(track, language))
                .unwrap_or_default(),
            Target::Song(_) => self.title.clone(),
        }
    }

    pub fn display_detail(&self, language: Language) -> String {
        let duration = duration_label(&self.tracks);
        let duration = tr(language, &duration);
        let count = self.tracks.len();
        let tracks = tr(language, if count == 1 { "track" } else { "tracks" });
        match &self.target {
            Target::Artist(_) => {
                let albums = self
                    .tracks
                    .iter()
                    .map(album_key)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                tr_message(
                    language,
                    "{albums} {album_label} · {count} {tracks} · {duration}",
                    &[
                        ("albums", albums.to_string()),
                        (
                            "album_label",
                            tr(language, if albums == 1 { "album" } else { "albums" }).into(),
                        ),
                        ("count", count.to_string()),
                        ("tracks", tracks.into()),
                        ("duration", duration.into()),
                    ],
                )
            }
            Target::Album(_) => format!(
                "{} · {count} {tracks} · {duration}",
                self.tracks
                    .first()
                    .map(|track| display_artist(track, language))
                    .unwrap_or_default()
            ),
            Target::Song(_) => self
                .tracks
                .first()
                .map(|track| {
                    format!(
                        "{}.{:02} · {} · {duration}",
                        track.disc.max(1),
                        track.track,
                        track.artist_or_folder()
                    )
                })
                .unwrap_or_default(),
        }
    }

    pub fn is_song(&self) -> bool {
        matches!(self.target, Target::Song(_))
    }
}

pub struct Browser {
    pub view: View,
    location: Location,
    history: Vec<(Location, ListState, Option<Target>)>,
    pub rows: Vec<Row>,
    pub state: ListState,
    pub title: String,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            view: View::Songs,
            location: Location::Songs,
            history: Vec::new(),
            rows: Vec::new(),
            state: ListState::default(),
            title: "Songs".into(),
        }
    }
}

fn artist(track: &LocalTrack) -> String {
    let name = if track.album_artist.trim().is_empty() {
        track.artist.trim()
    } else {
        track.album_artist.trim()
    };
    if name.is_empty() {
        "Unknown artist".into()
    } else {
        name.into()
    }
}

fn album_key(track: &LocalTrack) -> AlbumKey {
    AlbumKey {
        artist: artist(track).to_lowercase(),
        album: track.album.trim().to_lowercase(),
        folder: track.album.trim().is_empty().then(|| {
            track
                .path
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""))
                .into()
        }),
    }
}

fn album_name(track: &LocalTrack) -> String {
    if track.album.trim().is_empty() {
        if track.folder.is_empty() {
            "Unknown album".into()
        } else {
            track.folder.clone()
        }
    } else {
        track.album.trim().into()
    }
}

fn order_tracks(tracks: &mut [LocalTrack]) {
    tracks.sort_by_cached_key(|track| {
        (
            album_key(track),
            track.disc.max(1),
            track.track,
            track.path.clone(),
        )
    });
}

pub fn duration_label<'a>(tracks: impl IntoIterator<Item = &'a LocalTrack>) -> String {
    let mut total = Duration::ZERO;
    let mut unknown = false;
    for track in tracks {
        match track.duration {
            Some(duration) => total += duration,
            None => unknown = true,
        }
    }
    if unknown {
        if total.is_zero() {
            "duration unknown".into()
        } else {
            format!("{}:{:02}+", total.as_secs() / 60, total.as_secs() % 60)
        }
    } else {
        format!("{}:{:02}", total.as_secs() / 60, total.as_secs() % 60)
    }
}

fn display_artist(track: &LocalTrack, language: Language) -> String {
    if track.artist.is_empty() && track.album_artist.is_empty() {
        tr(language, "Unknown artist").into()
    } else {
        artist(track)
    }
}

fn display_album(track: &LocalTrack, language: Language) -> String {
    if track.album.is_empty() && track.folder.is_empty() {
        tr(language, "Unknown album").into()
    } else {
        album_name(track)
    }
}

impl Browser {
    pub fn heading(&self, language: Language) -> String {
        match &self.location {
            Location::Songs => tr(language, "Songs").into(),
            Location::Artists => tr(language, "Artists").into(),
            Location::Albums => tr(language, "Albums").into(),
            Location::Artist(_) => self.title.clone(),
            Location::Album(_) => self
                .rows
                .first()
                .and_then(|row| row.tracks.first())
                .map(|track| {
                    format!(
                        "{} — {}",
                        display_artist(track, language),
                        display_album(track, language)
                    )
                })
                .unwrap_or_else(|| tr(language, "Album").into()),
        }
    }

    pub fn is_songs(&self) -> bool {
        self.location == Location::Songs
    }
    pub fn is_album(&self) -> bool {
        matches!(self.location, Location::Album(_))
    }
    pub fn selected(&self) -> Option<&Row> {
        self.state.selected().and_then(|i| self.rows.get(i))
    }

    pub fn set_view(&mut self, view: View, tracks: &[LocalTrack]) {
        self.view = view;
        self.location = match view {
            View::Songs => Location::Songs,
            View::Artists => Location::Artists,
            View::Albums => Location::Albums,
        };
        self.history.clear();
        self.state = ListState::default();
        self.rows.clear();
        self.rebuild(tracks);
    }

    pub fn cycle(&mut self, tracks: &[LocalTrack]) {
        let view = match self.view {
            View::Songs => View::Artists,
            View::Artists => View::Albums,
            View::Albums => View::Songs,
        };
        self.set_view(view, tracks);
    }

    /// Opens a group. Song rows are handled by the playback controller.
    pub fn open(&mut self, tracks: &[LocalTrack]) -> bool {
        let location = match self.selected().map(|row| &row.target) {
            Some(Target::Artist(name)) => Location::Artist(name.clone()),
            Some(Target::Album(key)) => Location::Album(key.clone()),
            _ => return false,
        };
        let target = self.selected().map(|row| row.target.clone());
        self.history
            .push((self.location.clone(), self.state, target));
        self.location = location;
        self.rows.clear();
        self.state = ListState::default();
        self.rebuild(tracks);
        true
    }

    pub fn back(&mut self, tracks: &[LocalTrack]) -> bool {
        let Some((location, state, target)) = self.history.pop() else {
            if self.is_songs() {
                return false;
            }
            self.set_view(View::Songs, tracks);
            return true;
        };
        self.location = location;
        self.rows.clear();
        self.state = state;
        self.rebuild(tracks);
        if let Some(index) =
            target.and_then(|target| self.rows.iter().position(|row| row.target == target))
        {
            self.state.select(Some(index));
        }
        true
    }

    pub fn album_tracks(&self) -> Vec<LocalTrack> {
        if !self.is_album() {
            return Vec::new();
        }
        self.rows
            .iter()
            .flat_map(|row| row.tracks.iter().cloned())
            .collect()
    }

    pub fn playback(&self, from_selected: bool) -> Option<Playback> {
        let playback = if self.is_album() {
            Playback {
                tracks: self.album_tracks(),
                index: if from_selected {
                    self.state.selected()?
                } else {
                    0
                },
                name: self.title.clone(),
            }
        } else {
            let row = self.selected()?;
            Playback {
                tracks: row.tracks.clone(),
                index: 0,
                name: row.title.clone(),
            }
        };
        (playback.index < playback.tracks.len()).then_some(playback)
    }

    pub fn play_group(
        &self,
        selected_song: Option<&LocalTrack>,
        library: &[LocalTrack],
    ) -> Option<Playback> {
        if !self.is_songs() {
            return self.playback(false);
        }
        let selected = selected_song?;
        let key = album_key(selected);
        let mut tracks: Vec<_> = library
            .iter()
            .filter(|track| album_key(track) == key)
            .cloned()
            .collect();
        order_tracks(&mut tracks);
        if tracks.is_empty() {
            return None;
        }
        Some(Playback {
            tracks,
            index: 0,
            name: format!("{} — {}", artist(selected), album_name(selected)),
        })
    }

    pub fn rebuild(&mut self, tracks: &[LocalTrack]) {
        let keep = self.selected().map(|row| row.target.clone());
        let mut groups: BTreeMap<AlbumKey, Vec<LocalTrack>> = BTreeMap::new();
        self.rows = match &self.location {
            Location::Songs => {
                self.title = "Songs".into();
                Vec::new()
            }
            Location::Artists => {
                self.title = "Artists".into();
                let mut artists: BTreeMap<String, Vec<LocalTrack>> = BTreeMap::new();
                for track in tracks {
                    artists
                        .entry(artist(track).to_lowercase())
                        .or_default()
                        .push(track.clone());
                }
                artists
                    .into_iter()
                    .map(|(key, mut tracks)| {
                        order_tracks(&mut tracks);
                        let albums = tracks
                            .iter()
                            .map(album_key)
                            .collect::<std::collections::BTreeSet<_>>()
                            .len();
                        Row {
                            target: Target::Artist(key),
                            title: artist(&tracks[0]),
                            detail: format!(
                                "{albums} album{} · {} track{} · {}",
                                if albums == 1 { "" } else { "s" },
                                tracks.len(),
                                if tracks.len() == 1 { "" } else { "s" },
                                duration_label(&tracks)
                            ),
                            tracks,
                        }
                    })
                    .collect()
            }
            Location::Albums | Location::Artist(_) => {
                self.title = match &self.location {
                    Location::Artist(key) => tracks
                        .iter()
                        .find(|t| artist(t).to_lowercase() == *key)
                        .map(artist)
                        .unwrap_or_else(|| key.clone()),
                    _ => "Albums".into(),
                };
                for track in tracks {
                    if let Location::Artist(key) = &self.location
                        && artist(track).to_lowercase() != *key
                    {
                        continue;
                    }
                    groups
                        .entry(album_key(track))
                        .or_default()
                        .push(track.clone());
                }
                let mut rows: Vec<Row> = groups
                    .into_iter()
                    .map(|(key, mut tracks)| {
                        order_tracks(&mut tracks);
                        Row {
                            target: Target::Album(key),
                            title: album_name(&tracks[0]),
                            detail: format!(
                                "{} · {} track{} · {}",
                                artist(&tracks[0]),
                                tracks.len(),
                                if tracks.len() == 1 { "" } else { "s" },
                                duration_label(&tracks)
                            ),
                            tracks,
                        }
                    })
                    .collect();
                rows.sort_by_cached_key(|row| {
                    (row.title.to_lowercase(), row.detail.to_lowercase())
                });
                rows
            }
            Location::Album(key) => {
                let mut songs: Vec<_> = tracks
                    .iter()
                    .filter(|t| album_key(t) == *key)
                    .cloned()
                    .collect();
                order_tracks(&mut songs);
                self.title = songs
                    .first()
                    .map(|t| format!("{} — {}", artist(t), album_name(t)))
                    .unwrap_or_else(|| "Album".into());
                songs
                    .into_iter()
                    .map(|track| Row {
                        target: Target::Song(track.path.clone()),
                        title: track.title.clone(),
                        detail: format!(
                            "{}.{:02} · {} · {}",
                            track.disc.max(1),
                            track.track,
                            track.artist_or_folder(),
                            duration_label(std::slice::from_ref(&track))
                        ),
                        tracks: vec![track],
                    })
                    .collect()
            }
        };
        let selection = keep
            .and_then(|target| self.rows.iter().position(|row| row.target == target))
            .or_else(|| {
                (!self.rows.is_empty())
                    .then(|| self.state.selected().unwrap_or(0).min(self.rows.len() - 1))
            });
        self.state.select(selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(path: &str, artist: &str, album: &str, disc: u32, number: u32) -> LocalTrack {
        let mut track = crate::library::infrastructure::scanner::to_track(path.into());
        track.artist = artist.into();
        track.album = album.into();
        track.disc = disc;
        track.track = number;
        track.duration = Some(Duration::from_secs(90));
        track
    }

    #[test]
    fn albums_separate_artists_and_order_discs_then_tracks() {
        let tracks = vec![
            song("/a/last.wav", "Band", "Live", 2, 1),
            song("/a/second.wav", "Band", "Live", 1, 2),
            song("/b/other.wav", "Other", "Live", 1, 1),
            song("/a/first.wav", "Band", "Live", 0, 1),
        ];
        let mut browser = Browser::default();
        browser.set_view(View::Albums, &tracks);
        assert_eq!(browser.rows.len(), 2);
        let album = &browser.rows[0];
        assert_eq!(album.tracks.len(), 3);
        assert!(album.detail.contains("4:30"));
        assert!(browser.open(&tracks));
        let paths: Vec<_> = browser.album_tracks().into_iter().map(|t| t.path).collect();
        assert_eq!(
            paths,
            ["/a/first.wav", "/a/second.wav", "/a/last.wav"].map(PathBuf::from)
        );
        assert!(browser.rows.iter().all(Row::is_song));
        assert!(!browser.open(&tracks));
    }

    #[test]
    fn compilation_uses_album_artist_and_preserves_track_artists() {
        let mut tracks = vec![
            song("/mix/b.wav", "Singer B", "Mix", 1, 2),
            song("/mix/a.wav", "Singer A", "Mix", 1, 1),
        ];
        for track in &mut tracks {
            track.album_artist = "Various Artists".into();
        }
        let mut browser = Browser::default();
        browser.set_view(View::Artists, &tracks);
        assert_eq!(browser.rows.len(), 1);
        assert_eq!(browser.rows[0].title, "Various Artists");
        assert!(browser.open(&tracks));
        assert_eq!(browser.rows.len(), 1);
        assert!(browser.open(&tracks));
        assert_eq!(browser.rows[0].tracks[0].artist, "Singer A");
        assert_eq!(browser.rows[1].tracks[0].artist, "Singer B");
    }

    #[test]
    fn untagged_albums_are_separated_by_folder() {
        let tracks = vec![
            song("/one/track.wav", "", "", 0, 0),
            song("/two/track.wav", "", "", 0, 0),
        ];
        let mut browser = Browser::default();
        browser.set_view(View::Artists, &tracks);
        assert_eq!(browser.rows[0].title, "Unknown artist");
        browser.open(&tracks);
        assert_eq!(browser.rows.len(), 2);
        assert_eq!(browser.rows[0].title, "one");
        assert_eq!(browser.rows[1].title, "two");
    }

    #[test]
    fn back_restores_group_identity_after_rescan() {
        let mut tracks = vec![
            song("/b/one.wav", "Beta", "First", 1, 1),
            song("/c/one.wav", "Charlie", "Second", 1, 1),
        ];
        let mut browser = Browser::default();
        browser.set_view(View::Artists, &tracks);
        browser.state.select(Some(1));
        browser.open(&tracks);
        tracks.push(song("/a/one.wav", "Alpha", "New", 1, 1));
        browser.rebuild(&tracks);
        assert!(browser.back(&tracks));
        assert_eq!(browser.selected().unwrap().title, "Charlie");
        assert_eq!(browser.state.selected(), Some(2));
        assert!(browser.back(&tracks));
        assert!(browser.is_songs());
        assert!(!browser.back(&tracks));
    }

    #[test]
    fn filtering_and_rescanning_keep_selection_or_clamp_it() {
        let tracks = vec![
            song("/a/one.wav", "Artist", "Album", 1, 1),
            song("/a/two.wav", "Artist", "Album", 1, 2),
        ];
        let mut browser = Browser::default();
        browser.set_view(View::Albums, &tracks);
        browser.open(&tracks);
        browser.state.select(Some(1));
        browser.rebuild(&tracks[1..]);
        assert_eq!(browser.state.selected(), Some(0));
        assert_eq!(browser.selected().unwrap().tracks[0].path, tracks[1].path);
        browser.rebuild(&[]);
        assert!(browser.rows.is_empty());
        assert!(browser.state.selected().is_none());
        browser.rebuild(&tracks);
        assert_eq!(browser.state.selected(), Some(0));
    }

    #[test]
    fn duration_never_presents_partial_metadata_as_complete() {
        let mut tracks = vec![
            song("/a.wav", "A", "A", 1, 1),
            song("/b.wav", "A", "A", 1, 2),
        ];
        assert_eq!(duration_label(&tracks), "3:00");
        tracks[1].duration = None;
        assert_eq!(duration_label(&tracks), "1:30+");
        tracks[0].duration = None;
        assert_eq!(duration_label(&tracks), "duration unknown");
    }

    #[test]
    fn view_cycle_and_case_insensitive_grouping() {
        let tracks = vec![
            song("/one.wav", "Artist", "Album", 1, 1),
            song("/two.wav", " artist ", " album ", 1, 2),
        ];
        let mut browser = Browser::default();
        browser.cycle(&tracks);
        assert_eq!(browser.view, View::Artists);
        assert_eq!(browser.rows.len(), 1);
        browser.cycle(&tracks);
        assert_eq!(browser.view, View::Albums);
        assert_eq!(browser.rows.len(), 1);
        browser.cycle(&tracks);
        assert!(browser.is_songs());
        assert!(browser.rows.is_empty());
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;

    #[test]
    fn group_playback_and_selected_song_use_complete_ordered_album() {
        let mut tracks: Vec<_> = [3, 1, 2]
            .into_iter()
            .map(|number| {
                let mut track = crate::library::infrastructure::scanner::to_track(
                    format!("/album/{number}.wav").into(),
                );
                track.artist = "Artist".into();
                track.album = "Album".into();
                track.track = number;
                track
            })
            .collect();
        let mut browser = Browser::default();
        browser.set_view(View::Albums, &tracks);
        let group = browser.playback(false).unwrap();
        assert_eq!(group.index, 0);
        assert_eq!(
            group.tracks.iter().map(|t| t.track).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        browser.open(&tracks);
        browser.state.select(Some(1));
        let song = browser.playback(true).unwrap();
        assert_eq!(song.index, 1);
        assert_eq!(song.tracks[song.index].track, 2);
        assert_eq!(song.tracks.len(), 3);
        assert_eq!(browser.playback(false).unwrap().index, 0);
        // The playback context survives a later filter or library refresh.
        tracks.clear();
        browser.rebuild(&tracks);
        assert!(browser.playback(true).is_none());
        assert_eq!(song.tracks.len(), 3);
    }
}

#[cfg(test)]
mod songs_playback_tests {
    use super::*;

    #[test]
    fn play_group_in_songs_starts_selected_songs_whole_album() {
        let mut library: Vec<_> = [3, 1, 2]
            .into_iter()
            .map(|number| {
                let mut track = crate::library::infrastructure::scanner::to_track(
                    format!("/album/{number}.wav").into(),
                );
                track.album = "Album".into();
                track.artist = "Artist".into();
                track.track = number;
                track
            })
            .collect();
        let mut other = library[0].clone();
        other.artist = "Other artist".into();
        other.path = "/other/track.wav".into();
        library.push(other);
        let browser = Browser::default();
        let plan = browser.play_group(Some(&library[0]), &library).unwrap();
        assert_eq!(plan.index, 0);
        assert_eq!(
            plan.tracks
                .iter()
                .map(|track| track.track)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(plan.name.contains("Album"));
        assert!(browser.play_group(None, &library).is_none());
        assert!(browser.play_group(Some(&library[0]), &[]).is_none());
    }

    #[test]
    fn play_group_in_songs_uses_folder_when_album_tag_is_missing() {
        let library: Vec<_> = ["/one/a.wav", "/two/a.wav", "/one/b.wav"]
            .into_iter()
            .map(|path| crate::library::infrastructure::scanner::to_track(path.into()))
            .collect();
        let plan = Browser::default()
            .play_group(Some(&library[0]), &library)
            .unwrap();
        assert_eq!(plan.tracks.len(), 2);
        assert!(plan.tracks.iter().all(|track| track.folder == "one"));
    }
}
