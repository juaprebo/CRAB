use crate::analyzer::Spectrum;
use crate::art::Art;
use crate::commands::{self, SeekTo};
use crate::config::{self, Config};
use crate::cover::{self, Cover};
use crate::engine::{Engine, Track};
use crate::finder::{self, Finder, FinderOutcome};
use crate::keys::{self, Action, Key, Keymap};
use crate::library::{self, LocalTrack, Scan};
use crate::library_browser::{Browser, View};
use crate::lyrics::{self, Lyrics};
use crate::media::{self, Media};
use crate::picker::{Outcome, Picker};
use crate::playlist::{self, Playlist};
use crate::radio::{self, Station};
use crate::scrobble::{self, Listen, Scrobbler};
use crate::spotify::{self, SpotifyCommand, SpotifyEvent, SpotifyHandle};
use crate::state::{self, State};
use crate::stats::Stats;
use crate::sysvol::{self, Request, SystemVolume};
use crate::theme::Theme;
use crate::theme::rgb;
use crate::theme_editor::{
    Editor, EditorOutcome, Generator, GeneratorOutcome, PickerOutcome, ThemePicker,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    Playlists,
    Radio,
    Spotify,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Library, Tab::Playlists, Tab::Radio, Tab::Spotify];

    pub fn index(self) -> usize {
        match self {
            Tab::Library => 0,
            Tab::Playlists => 1,
            Tab::Radio => 2,
            Tab::Spotify => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Library => "Library",
            Tab::Playlists => "Playlists",
            Tab::Radio => "Radio",
            Tab::Spotify => "Spotify",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Active {
    None,
    Local(usize),
    Queue(usize),
    Extra,
    Radio(usize),
    Spotify,
}

#[derive(Clone, PartialEq, Eq)]
pub enum SpotifyStatus {
    LoggedOut,
    Connecting,
    Ready,
    Failed(String),
}

pub struct SpotifyState {
    pub status: SpotifyStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub position_ms: u32,
    pub position_at: Instant,
    pub playing: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Repeat {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn from_name(name: &str) -> Repeat {
        match name {
            "all" => Repeat::All,
            "one" => Repeat::One,
            _ => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }
}

#[derive(Clone)]
enum Resume {
    Local(PathBuf),
    Queue(usize),
}

struct CmdCycle {
    base: String,
    list: Vec<String>,
    index: usize,
}

struct Visual {
    anchor: usize,
    base: HashSet<PathBuf>,
    base_index: HashSet<usize>,
}

struct Tracking {
    listen: Listen,
    played: Duration,
    last: Instant,
    sent: bool,
}

struct Pick {
    track: LocalTrack,
    active: Active,
    pop: bool,
    resume: Option<Resume>,
    duration: Option<Duration>,
    epoch: u64,
}

impl Pick {
    fn resolve_active(&self, tracks: &[LocalTrack]) -> Active {
        // A view may have been sorted, filtered or rescanned since preloading.
        match self.active {
            Active::Local(_) => tracks
                .iter()
                .position(|track| track.path == self.track.path)
                .map_or(Active::Extra, Active::Local),
            active => active,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    SaveMarked,
    NewList,
    ImportM3u,
    ExportM3u(usize),
}

#[derive(Default)]
pub struct Hit {
    pub body: Rect,
    pub progress: Rect,
    pub tabs: Vec<(Rect, Tab)>,
}

pub struct NamePrompt {
    pub title: &'static str,
    pub label: &'static str,
    pub input: String,
    pub paths: Vec<PathBuf>,
    pub kind: PromptKind,
}

type CoverResult = (PathBuf, Option<Cover>, Option<PathBuf>);

type RadioResult = (u64, std::result::Result<Track, String>);

pub struct App {
    pub engine: Engine,
    pub spectrum: Spectrum,
    pub tracks: Vec<LocalTrack>,
    all_tracks: Vec<LocalTrack>,
    pub filter: String,
    pub filtering: bool,
    pub repeat: Repeat,
    pub up_next: VecDeque<LocalTrack>,
    resume: Option<Resume>,
    pub queue_view: Option<ListState>,
    pub help: Option<usize>,
    pub hit: Hit,
    last_click: Option<(Instant, usize)>,
    sleep_at: Option<Instant>,
    restore: Option<(PathBuf, Duration)>,
    restore_shown: bool,
    current_path: Option<PathBuf>,
    saved_at: Instant,
    pub scanning: bool,
    pub stations: Vec<Station>,
    pub tab: Tab,
    pub library_state: ListState,
    pub browser: Browser,
    pub browser_cover: Option<Cover>,
    pub native_covers: bool,
    pub cover_area: Option<Rect>,
    spotify_cover: Option<Cover>,
    spotify_cover_rx: Option<Receiver<Option<Cover>>>,
    browser_cover_for: Option<PathBuf>,
    browser_cover_rx: Option<Receiver<Option<Cover>>>,
    pub radio_state: ListState,
    pub active: Active,
    pub loading: bool,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub radio_title: Arc<Mutex<String>>,
    pub status: String,
    pub status_is_error: bool,
    status_at: Instant,
    pub shuffle: bool,
    pub spotify: Option<SpotifyHandle>,
    pub sp: SpotifyState,
    pub quit: bool,
    pub picker: Option<Picker>,
    pub theme_picker: Option<ThemePicker>,
    pub theme_editor: Option<Editor>,
    pub theme_generator: Option<Generator>,
    pub finder: Option<Finder>,
    pub playlists: Vec<Playlist>,
    pub playlist_state: ListState,
    pub open_playlist: Option<usize>,
    pub playlist_track_state: ListState,
    pub marked: HashSet<PathBuf>,
    pub marked_tracks: HashSet<usize>,
    pub name_prompt: Option<NamePrompt>,
    queue: Vec<LocalTrack>,
    queue_name: String,
    queue_is_playlist: bool,
    pending_remove: Option<usize>,
    pub config: Config,
    pub themes: Vec<Theme>,
    pub theme_index: usize,
    pub art: Option<Art>,
    sysvol: Option<SystemVolume>,
    volume: f32,
    pub muted: bool,
    tracks_rx: Option<Receiver<Scan>>,
    meta: HashMap<PathBuf, LocalTrack>,
    pub cover: Option<Cover>,
    cover_tx: Sender<CoverResult>,
    cover_rx: Receiver<CoverResult>,
    preloaded: Option<Pick>,
    pub keys: Keymap,
    stats: Option<Stats>,
    count: Option<usize>,
    pending_g: bool,
    visual: Option<Visual>,
    pub cmdline: Option<String>,
    cmd_cycle: Option<CmdCycle>,
    cmd_history: Vec<String>,
    cmd_pos: Option<usize>,
    cmd_draft: String,
    pub favorites: BTreeSet<PathBuf>,
    pub only_favorites: bool,
    pub lyrics: Option<Lyrics>,
    pub lyrics_view: Option<usize>,
    scrobbler: Option<Scrobbler>,
    tracking: Option<Tracking>,
    preload_skip: Option<PathBuf>,
    cover_file: Option<PathBuf>,
    cover_serial: u64,
    media: Option<Media>,
    cover_for: Option<PathBuf>,
    pub album: String,
    roots: Vec<PathBuf>,
    watcher: Option<(notify::RecommendedWatcher, Receiver<()>)>,
    rescan_at: Option<Instant>,
    radio_tx: Sender<RadioResult>,
    radio_rx: Receiver<RadioResult>,
    generation: u64,
    radio_started: Instant,
    rng: u64,
}

fn playlist_state(playlists: &[Playlist]) -> ListState {
    let mut state = ListState::default();
    if !playlists.is_empty() {
        state.select(Some(0));
    }
    state
}

impl App {
    pub fn new(mut engine: Engine, roots: Vec<PathBuf>) -> Self {
        let (config, mut warning) = Config::load();
        engine.eq.set(&config.audio.eq_gains());
        let (key_map, key_warnings) = config.keymap();
        let themes = config.build_themes();
        let theme_index = themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&config.theme))
            .unwrap_or(0);
        let art = config.load_art();
        let sysvol = sysvol::start();
        let saved = state::load();
        let (volume, muted) = match &sysvol {
            Some(sys) => {
                engine.set_volume(1.0);
                (sys.initial.volume, sys.initial.muted)
            }
            None => {
                if let Some(volume) = saved.volume {
                    engine.set_volume(volume);
                }
                (engine.volume(), false)
            }
        };
        let (radio_tx, radio_rx) = channel();
        let stations = match radio::load_stations() {
            Ok(stations) => stations,
            Err(error) => {
                let message = format!("{error:#}");
                warning = Some(match warning {
                    Some(previous) => format!("{previous}; {message}"),
                    None => message,
                });
                Vec::new()
            }
        };
        let mut radio_state = ListState::default();
        if !stations.is_empty() {
            radio_state.select(Some(0));
        }
        let (spotify_handle, spotify_status) = if spotify::has_credentials() {
            (
                Some(spotify::start(engine.spotify_sender())),
                SpotifyStatus::Connecting,
            )
        } else {
            (None, SpotifyStatus::LoggedOut)
        };
        let playlists = playlist::load();
        let playlists_paths: Vec<PathBuf> = playlists
            .iter()
            .flat_map(|p| p.tracks.iter().cloned())
            .collect();
        let sort = config.library.sort;
        let (cover_tx, cover_rx) = channel();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;
        let mut app = Self {
            engine,
            spectrum: Spectrum::new(),
            tracks: Vec::new(),
            all_tracks: Vec::new(),
            filter: String::new(),
            filtering: false,
            repeat: Repeat::from_name(&saved.repeat),
            up_next: VecDeque::new(),
            resume: None,
            queue_view: None,
            help: None,
            hit: Hit::default(),
            last_click: None,
            sleep_at: None,
            restore: saved
                .track
                .clone()
                .map(|p| (p, Duration::from_secs(saved.position_secs))),
            restore_shown: false,
            current_path: None,
            saved_at: Instant::now(),
            scanning: true,
            stations,
            tab: Tab::Library,
            library_state: ListState::default(),
            browser: Browser::default(),
            browser_cover: None,
            native_covers: false,
            cover_area: None,
            spotify_cover: None,
            spotify_cover_rx: None,
            browser_cover_for: None,
            browser_cover_rx: None,
            radio_state,
            active: Active::None,
            loading: false,
            title: String::new(),
            artist: String::new(),
            duration: None,
            radio_title: Arc::new(Mutex::new(String::new())),
            status: warning.clone().unwrap_or_default(),
            status_is_error: warning.is_some(),
            status_at: Instant::now(),
            shuffle: saved.shuffle,
            spotify: spotify_handle,
            sp: SpotifyState {
                status: spotify_status,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                duration_ms: 0,
                position_ms: 0,
                position_at: Instant::now(),
                playing: false,
            },
            quit: false,
            picker: None,
            theme_picker: None,
            theme_editor: None,
            theme_generator: None,
            finder: None,
            playlist_state: playlist_state(&playlists),
            playlists,
            open_playlist: None,
            playlist_track_state: ListState::default(),
            marked: HashSet::new(),
            marked_tracks: HashSet::new(),
            name_prompt: None,
            queue: Vec::new(),
            queue_name: String::new(),
            queue_is_playlist: false,
            pending_remove: None,
            config,
            themes,
            theme_index,
            art,
            sysvol,
            volume,
            muted,
            tracks_rx: Some(library::scan(roots.clone(), playlists_paths, sort)),
            meta: HashMap::new(),
            cover: None,
            cover_tx,
            cover_rx,
            cover_for: None,
            preloaded: None,
            keys: key_map,
            stats: None,
            count: None,
            pending_g: false,
            visual: None,
            cmdline: None,
            cmd_cycle: None,
            cmd_history: Vec::new(),
            cmd_pos: None,
            cmd_draft: String::new(),
            favorites: saved.favorites.iter().cloned().collect(),
            only_favorites: false,
            lyrics: None,
            lyrics_view: None,
            scrobbler: None,
            tracking: None,
            preload_skip: None,
            cover_file: None,
            cover_serial: 0,
            media: Media::start(),
            album: String::new(),
            watcher: library::watch(&roots),
            roots,
            rescan_at: None,
            radio_tx,
            radio_rx,
            generation: 0,
            radio_started: Instant::now(),
            rng: seed,
        };
        app.start_scrobbler();
        if !key_warnings.is_empty() {
            app.set_status(key_warnings.join("; "), true);
        }
        app
    }

    pub fn playing_playlist(&self, name: &str) -> bool {
        self.queue_is_playlist && matches!(self.active, Active::Queue(_)) && self.queue_name == name
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = message.into();
        self.status_is_error = is_error;
        self.status_at = Instant::now();
    }

    fn expire_status(&mut self) {
        let lifetime = if self.status_is_error {
            Duration::from_secs(8)
        } else {
            Duration::from_secs(4)
        };
        if !self.status.is_empty() && self.status_at.elapsed() >= lifetime {
            self.status.clear();
            self.status_is_error = false;
        }
    }

    fn next_random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    pub fn theme(&self) -> &Theme {
        if let Some(editor) = &self.theme_editor {
            return &editor.draft;
        }
        if let Some(generator) = &self.theme_generator {
            return &generator.draft;
        }
        &self.themes[self.theme_index]
    }

    pub fn volume(&self) -> f32 {
        self.volume.clamp(0.0, 1.0)
    }

    fn change_volume(&mut self, delta: f32) {
        match &self.sysvol {
            Some(sys) => {
                self.volume = (self.volume + delta).clamp(0.0, 1.0);
                sys.send(Request::Set(self.volume));
                if self.muted {
                    sys.send(Request::ToggleMute);
                    self.muted = false;
                }
            }
            None => {
                self.engine.change_volume(delta);
                self.volume = self.engine.volume();
            }
        }
    }

    fn toggle_mute(&mut self) {
        match &self.sysvol {
            Some(sys) => {
                sys.send(Request::ToggleMute);
                self.muted = !self.muted;
            }
            None => {
                if self.muted {
                    self.engine.set_volume(self.volume);
                } else {
                    self.engine.set_volume(0.0);
                }
                self.muted = !self.muted;
            }
        }
    }

    fn open_theme_picker(&mut self) {
        self.theme_picker = Some(ThemePicker::new(self.theme_index));
    }

    fn free_theme_name(&self) -> String {
        let taken = |name: &str| {
            self.themes
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(name))
        };
        let mut name = "my-theme".to_string();
        let mut counter = 2;
        while taken(&name) {
            name = format!("my-theme-{counter}");
            counter += 1;
        }
        name
    }

    fn theme_picker_key(&mut self, key: KeyEvent) {
        let count = self.themes.len();
        let Some(picker) = self.theme_picker.as_mut() else {
            return;
        };
        let outcome = picker.handle_key(key, count);
        let selected = picker.selected().min(count.saturating_sub(1));
        let original = picker.original;
        match outcome {
            PickerOutcome::Stay => self.theme_index = selected,
            PickerOutcome::Cancel => {
                self.theme_index = original;
                self.theme_picker = None;
            }
            PickerOutcome::Apply => {
                self.theme_index = selected;
                self.theme_picker = None;
                let name = self.theme().name.clone();
                match config::pick_theme(&name) {
                    Ok(()) => {
                        self.config.theme = name.clone();
                        self.set_status(format!("Theme: {name}"), false);
                    }
                    Err(error) => {
                        self.set_status(format!("Theme {name} not saved: {error:#}"), true)
                    }
                }
            }
            PickerOutcome::Delete => {
                let name = self.themes[selected].name.clone();
                let custom = self
                    .config
                    .themes
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case(&name));
                if let Some(picker) = self.theme_picker.as_mut() {
                    if custom {
                        picker.confirm = true;
                    } else {
                        picker.notice = Some("Built-in themes cannot be deleted".to_string());
                    }
                }
            }
            PickerOutcome::ConfirmDelete => {
                let name = self.themes[selected].name.clone();
                match config::delete_theme(&name) {
                    Ok(()) => {
                        self.reload_config();
                        let applied = self
                            .themes
                            .iter()
                            .position(|t| t.name.eq_ignore_ascii_case(&self.config.theme))
                            .unwrap_or(0);
                        let target = selected.min(self.themes.len() - 1);
                        self.theme_index = target;
                        if let Some(picker) = self.theme_picker.as_mut() {
                            picker.state.select(Some(target));
                            picker.original = applied;
                        }
                        self.set_status(format!("Deleted theme: {name}"), false);
                    }
                    Err(error) => {
                        if let Some(picker) = self.theme_picker.as_mut() {
                            picker.notice = Some(format!("{error:#}"));
                        }
                    }
                }
            }
            PickerOutcome::New => {
                self.theme_index = selected;
                self.theme_picker = None;
                let accent = rgb(self.themes[selected].accent);
                let dark = self.themes[selected].background.is_none_or(|c| {
                    let (r, g, b) = rgb(c);
                    (r as u32 + g as u32 + b as u32) < 384
                });
                self.theme_generator = Some(Generator::new(accent, dark, original));
            }
            PickerOutcome::Edit => {
                let base = self.themes[selected].clone();
                let name = if self
                    .config
                    .themes
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case(&base.name))
                {
                    base.name.clone()
                } else {
                    format!("{}-custom", base.name)
                };
                self.theme_index = selected;
                self.theme_picker = None;
                self.theme_editor = Some(Editor::new(&base, name, original));
            }
        }
    }

    fn theme_generator_key(&mut self, key: KeyEvent) {
        let Some(generator) = self.theme_generator.as_mut() else {
            return;
        };
        match generator.handle_key(key) {
            GeneratorOutcome::Stay => {}
            GeneratorOutcome::Cancel => {
                if let Some(generator) = self.theme_generator.take() {
                    self.theme_index = generator.original.min(self.themes.len() - 1);
                }
            }
            GeneratorOutcome::Accept => {
                if let Some(generator) = self.theme_generator.take() {
                    let name = self.free_theme_name();
                    self.theme_editor =
                        Some(Editor::new(&generator.draft, name, generator.original));
                }
            }
        }
    }

    fn theme_editor_key(&mut self, key: KeyEvent) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        match editor.handle_key(key) {
            EditorOutcome::Stay => {}
            EditorOutcome::Cancel => {
                if let Some(editor) = self.theme_editor.take() {
                    self.theme_index = editor.original.min(self.themes.len() - 1);
                }
            }
            EditorOutcome::Save(name) => {
                let section = editor.draft.to_toml_section(&name);
                match config::save_theme(&name, &section) {
                    Ok(()) => {
                        self.theme_editor = None;
                        self.reload_config();
                        self.set_status(format!("Theme saved: {name}"), false);
                    }
                    Err(error) => {
                        editor.naming = None;
                        editor.error = Some(format!("{error:#}"));
                    }
                }
            }
        }
    }

    fn reload_config(&mut self) {
        let (config, warning) = Config::load();
        let current = self.theme().name.clone();
        self.themes = config.build_themes();
        self.theme_index = self
            .themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&config.theme))
            .or_else(|| self.themes.iter().position(|t| t.name == current))
            .unwrap_or(0);
        self.art = config.load_art();
        self.config = config;
        let (key_map, key_warnings) = self.config.keymap();
        self.keys = key_map;
        self.apply_eq();
        self.start_scrobbler();
        match warning {
            Some(message) => self.set_status(message, true),
            None if !key_warnings.is_empty() => self.set_status(key_warnings.join("; "), true),
            None => self.set_status("Config reloaded", false),
        }
    }

    fn poll_volume(&mut self) {
        let Some(sys) = &self.sysvol else {
            return;
        };
        let mut latest = None;
        while let Ok(reading) = sys.readings.try_recv() {
            latest = Some(reading);
        }
        if let Some(reading) = latest {
            self.volume = reading.volume;
            self.muted = reading.muted;
        }
    }

    pub fn is_playing(&self) -> bool {
        match self.active {
            Active::None => false,
            Active::Spotify => self.sp.playing,
            _ => !self.loading && !self.engine.is_paused(),
        }
    }

    pub fn position(&self) -> Duration {
        match self.active {
            Active::Spotify => {
                let mut ms = self.sp.position_ms as u64;
                if self.sp.playing {
                    ms += self.sp.position_at.elapsed().as_millis() as u64;
                }
                Duration::from_millis(ms.min(self.sp.duration_ms as u64))
            }
            Active::None => Duration::ZERO,
            _ => self.engine.position(),
        }
    }

    pub fn total(&self) -> Option<Duration> {
        match self.active {
            Active::Spotify => Some(Duration::from_millis(self.sp.duration_ms as u64)),
            Active::Local(_) | Active::Queue(_) | Active::Extra => self.duration,
            _ => None,
        }
    }

    fn start_scan(&mut self) {
        let extra: Vec<PathBuf> = self
            .playlists
            .iter()
            .flat_map(|p| p.tracks.iter().cloned())
            .collect();
        self.tracks_rx = Some(library::scan(
            self.roots.clone(),
            extra,
            self.config.library.sort,
        ));
    }

    pub fn meta(&self, path: &PathBuf) -> Option<&LocalTrack> {
        self.meta.get(path)
    }

    pub fn current_track_path(&self) -> Option<&PathBuf> {
        if matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        ) {
            self.current_path.as_ref()
        } else {
            None
        }
    }

    fn resolve_track(&mut self, path: &PathBuf) -> LocalTrack {
        if let Some(track) = self.meta.get(path) {
            return track.clone();
        }
        let track = library::read_track(path.clone());
        self.meta.insert(path.clone(), track.clone());
        track
    }

    fn apply_tracks(&mut self, scan: Scan) {
        self.meta = scan
            .tracks
            .iter()
            .chain(scan.extra.iter())
            .map(|t| (t.path.clone(), t.clone()))
            .collect();
        if let Active::Local(i) = self.active {
            let now = self.tracks.get(i).map(|t| t.path.clone());
            let still_there = now
                .as_ref()
                .is_some_and(|p| scan.tracks.iter().any(|t| &t.path == p));
            if !still_there {
                self.engine.stop();
                self.active = Active::None;
            }
        }
        self.all_tracks = scan.tracks;
        self.rebuild_view();
        if !self.restore_shown
            && self.active == Active::None
            && let Some((path, _)) = &self.restore
        {
            self.restore_shown = true;
            if let Some(index) = self.tracks.iter().position(|t| &t.path == path) {
                self.library_state.select(Some(index));
            }
        }
    }

    fn matches_filter(track: &LocalTrack, words: &[String]) -> bool {
        let hay = format!(
            "{} {} {} {} {}",
            track.title, track.artist, track.album_artist, track.album, track.folder
        )
        .to_lowercase();
        words.iter().all(|w| hay.contains(w.as_str()))
    }

    fn rebuild_view(&mut self) {
        let keep = self
            .library_state
            .selected()
            .and_then(|i| self.tracks.get(i))
            .map(|t| t.path.clone());
        let now = match self.active {
            Active::Local(i) => self.tracks.get(i).map(|t| t.path.clone()),
            _ => None,
        };
        let words: Vec<String> = self
            .filter
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        let favorites_only = self.only_favorites;
        let tracks: Vec<LocalTrack> = self
            .all_tracks
            .iter()
            .filter(|t| {
                (words.is_empty() || Self::matches_filter(t, &words))
                    && (!favorites_only || self.favorites.contains(&t.path))
            })
            .cloned()
            .collect();
        self.tracks = tracks;
        let find = |path: &Option<PathBuf>, tracks: &[LocalTrack]| {
            path.as_ref()
                .and_then(|p| tracks.iter().position(|t| &t.path == p))
        };
        if now.is_some() {
            match find(&now, &self.tracks) {
                Some(index) => self.active = Active::Local(index),
                None => {
                    self.active = Active::Extra;
                    self.resume = None;
                }
            }
        }
        let selection = find(&keep, &self.tracks).or(if self.tracks.is_empty() {
            None
        } else {
            Some(0)
        });
        self.library_state.select(selection);
        self.browser.rebuild(&self.tracks);
        self.refresh_browser_cover();
    }

    pub fn displayed_cover(&self) -> Option<&Cover> {
        match self.tab {
            Tab::Library | Tab::Playlists => self.browser_cover.as_ref(),
            Tab::Spotify => self.spotify_cover.as_ref(),
            Tab::Radio => None,
        }
    }

    pub fn cover_target(&self) -> Option<Rect> {
        if self.overlay_open() {
            None
        } else {
            self.cover_area
        }
    }

    fn refresh_browser_cover(&mut self) {
        let path = match self.tab {
            Tab::Library if self.browser.is_album() => self
                .browser
                .rows
                .first()
                .and_then(|row| row.tracks.first())
                .map(|track| track.path.clone()),
            Tab::Library if self.browser.is_songs() => self
                .library_state
                .selected()
                .and_then(|i| self.tracks.get(i))
                .map(|track| track.path.clone()),
            Tab::Library => self
                .browser
                .selected()
                .and_then(|row| row.tracks.first())
                .map(|track| track.path.clone()),
            Tab::Playlists => {
                let list = self
                    .open_playlist
                    .or_else(|| self.playlist_state.selected())
                    .and_then(|i| self.playlists.get(i));
                let index = if self.open_playlist.is_some() {
                    self.playlist_track_state.selected().unwrap_or(0)
                } else {
                    0
                };
                list.and_then(|list| list.tracks.get(index)).cloned()
            }
            _ => None,
        };
        if path == self.browser_cover_for {
            return;
        }
        self.browser_cover_for = path.clone();
        self.browser_cover = None;
        self.browser_cover_rx = path.map(|path| {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let cover = library::read_cover(&path)
                    .as_deref()
                    .and_then(cover::decode);
                let _ = tx.send(cover);
            });
            rx
        });
    }

    fn set_library_view(&mut self, view: View) {
        self.visual = None;
        self.marked.clear();
        self.last_click = None;
        self.browser.set_view(view, &self.tracks);
        self.refresh_browser_cover();
    }

    fn cycle_library_view(&mut self) {
        self.visual = None;
        self.marked.clear();
        self.last_click = None;
        self.browser.cycle(&self.tracks);
        self.refresh_browser_cover();
    }

    fn library_back(&mut self) -> bool {
        if self.tab != Tab::Library {
            return false;
        }
        let back = self.browser.back(&self.tracks);
        if back {
            self.last_click = None;
            self.refresh_browser_cover();
        }
        back
    }

    fn play_browser(&mut self, open: bool) {
        if open && self.browser.open(&self.tracks) {
            self.last_click = None;
            self.refresh_browser_cover();
            return;
        }
        let selected_song = self
            .library_state
            .selected()
            .and_then(|index| self.tracks.get(index));
        let playback = if open {
            self.browser.playback(true)
        } else {
            self.browser.play_group(selected_song, &self.all_tracks)
        };
        let Some(playback) = playback else {
            self.set_status("Select a track, artist or album first", false);
            return;
        };
        let message = format!(
            "Playing {} ({} tracks)",
            playback.name,
            playback.tracks.len()
        );
        self.queue = playback.tracks;
        self.queue_name = playback.name;
        self.queue_is_playlist = false;
        self.resume = None;
        if self.play_queue(playback.index) {
            self.set_status(message, false);
        }
    }

    pub fn live_title(&self) -> String {
        self.radio_title.lock().unwrap().clone()
    }

    pub fn tick(&mut self, bars: usize) {
        self.refresh_browser_cover();
        if let Some(rx) = &self.spotify_cover_rx
            && let Ok(cover) = rx.try_recv()
        {
            self.spotify_cover = cover;
            self.spotify_cover_rx = None;
        }
        if let Some(rx) = &self.browser_cover_rx
            && let Ok(cover) = rx.try_recv()
        {
            self.browser_cover = cover;
            self.browser_cover_rx = None;
        }
        self.expire_status();
        self.check_sleep();
        self.autosave();
        self.drive_transitions();
        if let Some(stats) = self.stats.as_mut() {
            stats.refresh();
        }
        self.track_listen();
        if let Some(finder) = self.finder.as_mut() {
            finder.poll();
        }
        if let Some((_, rx)) = &self.watcher {
            let mut changed = false;
            while rx.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                self.rescan_at = Some(Instant::now() + Duration::from_millis(1500));
            }
        }
        if self.tracks_rx.is_none()
            && let Some(at) = self.rescan_at
            && Instant::now() >= at
        {
            self.rescan_at = None;
            self.start_scan();
        }

        while let Ok((path, cover, file)) = self.cover_rx.try_recv() {
            if self.cover_for.as_ref() == Some(&path) {
                self.cover = cover;
                if let Some(old) = self.cover_file.take() {
                    let _ = std::fs::remove_file(old);
                }
                self.cover_file = file;
            } else if let Some(file) = file {
                let _ = std::fs::remove_file(file);
            }
        }
        self.sync_media();

        if let Some(rx) = &self.tracks_rx
            && let Ok(scan) = rx.try_recv()
        {
            self.apply_tracks(scan);
            self.scanning = false;
            self.tracks_rx = None;
        }

        while let Ok((generation, result)) = self.radio_rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.loading = false;
            match result {
                Ok(track) => {
                    self.engine.play(track);
                    self.radio_started = Instant::now();
                    self.set_status("", false);
                }
                Err(message) => {
                    self.active = Active::None;
                    self.set_status(message, true);
                }
            }
        }

        let mut events = Vec::new();
        if let Some(handle) = &self.spotify {
            while let Ok(event) = handle.events.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.on_spotify_event(event);
        }

        if !self.loading {
            match self.active {
                Active::Local(index) if self.engine.is_finished() => {
                    if !self.start_queued() {
                        self.advance_local(index);
                    }
                }
                Active::Queue(index) if self.engine.is_finished() => {
                    if !self.start_queued() {
                        self.advance_queue(index);
                    }
                }
                Active::Extra if self.engine.is_finished() => {
                    if !self.start_queued() {
                        self.resume_after();
                    }
                }
                Active::Radio(index) if self.engine.is_finished() => {
                    if self.radio_started.elapsed() > Duration::from_secs(5) {
                        self.set_status("Stream interrupted, reconnecting", true);
                        self.play_radio(index);
                    } else {
                        self.active = Active::None;
                        self.set_status("Stream ended", true);
                    }
                }
                _ => {}
            }
        }

        self.poll_volume();
        let active = self.is_playing();
        self.spectrum.update(bars, &self.engine.tap, active);
    }

    fn on_spotify_event(&mut self, event: SpotifyEvent) {
        match event {
            SpotifyEvent::Ready => {
                self.sp.status = SpotifyStatus::Ready;
            }
            SpotifyEvent::Failed(message) => {
                self.sp.status = SpotifyStatus::Failed(message);
                self.sp.playing = false;
                if self.active == Active::Spotify {
                    self.active = Active::None;
                }
            }
            SpotifyEvent::Track {
                title,
                artist,
                album,
                duration_ms,
                cover_url,
            } => {
                self.spotify_cover = None;
                self.spotify_cover_rx = cover_url.map(|url| {
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(cover::download(&url));
                    });
                    rx
                });
                self.sp.title = title;
                self.sp.artist = artist;
                self.sp.album = album;
                self.sp.duration_ms = duration_ms;
                self.sp.position_ms = 0;
                self.sp.position_at = Instant::now();
            }
            SpotifyEvent::Playing(position) => {
                self.spotify_takes_over();
                self.sp.playing = true;
                self.sp.position_ms = position;
                self.sp.position_at = Instant::now();
            }
            SpotifyEvent::Paused(position) => {
                self.sp.playing = false;
                self.sp.position_ms = position;
                self.sp.position_at = Instant::now();
            }
            SpotifyEvent::Position(position) => {
                self.sp.position_ms = position;
                self.sp.position_at = Instant::now();
            }
            SpotifyEvent::Stopped => {
                self.sp.playing = false;
            }
        }
    }

    fn spotify_takes_over(&mut self) {
        if self.active != Active::Spotify {
            self.generation += 1;
            self.loading = false;
            self.engine.stop();
            self.active = Active::Spotify;
            self.set_status("", false);
        }
    }

    fn pause_spotify(&mut self) {
        if self.sp.playing {
            if let Some(handle) = &self.spotify {
                handle.send(SpotifyCommand::PlayPause);
            }
            self.sp.playing = false;
        }
    }

    fn open_track(&mut self, track: &LocalTrack, active: Active) -> bool {
        self.pause_spotify();
        self.generation += 1;
        self.loading = false;
        self.preloaded = None;
        let gain = self.gain_for(track);
        match Engine::open_file(&track.path, self.engine.tap(), self.engine.eq.clone(), gain) {
            Ok(source) => {
                self.duration = self.engine.play(source);
                self.active = active;
                self.title = track.title.clone();
                self.artist = track.artist_or_folder();
                self.album = track.album.clone();
                self.current_path = Some(track.path.clone());
                if let Some((path, position)) = self.restore.take()
                    && path == track.path
                    && position > Duration::from_secs(2)
                {
                    self.engine.seek_by(true, position, self.duration);
                }
                self.load_cover(&track.path);
                self.set_status("", false);
                true
            }
            Err(error) => {
                self.engine.stop();
                self.active = Active::None;
                self.set_status(format!("{error:#}"), true);
                false
            }
        }
    }

    fn start_scrobbler(&mut self) {
        let token = std::env::var("CRAB_LISTENBRAINZ_TOKEN")
            .unwrap_or_else(|_| self.config.scrobble.listenbrainz_token.clone());
        self.scrobbler = Scrobbler::start(&token);
    }

    fn begin_listen(&mut self) {
        self.tracking = None;
        let Some(scrobbler) = &self.scrobbler else {
            return;
        };
        if self.artist.is_empty() || self.title.is_empty() {
            return;
        }
        let listen = Listen {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            started: scrobble::now_secs(),
            duration: self.duration,
        };
        scrobbler.now_playing(listen.clone());
        self.tracking = Some(Tracking {
            listen,
            played: Duration::ZERO,
            last: Instant::now(),
            sent: false,
        });
    }

    fn track_listen(&mut self) {
        let playing = self.is_playing()
            && matches!(
                self.active,
                Active::Local(_) | Active::Queue(_) | Active::Extra
            );
        let Some(tracking) = self.tracking.as_mut() else {
            return;
        };
        let now = Instant::now();
        if playing {
            tracking.played += now - tracking.last;
        }
        tracking.last = now;
        if !tracking.sent && scrobble::eligible(tracking.listen.duration, tracking.played) {
            tracking.sent = true;
            if let Some(scrobbler) = &self.scrobbler {
                scrobbler.listened(tracking.listen.clone());
            }
        }
    }

    fn gain_for(&self, track: &LocalTrack) -> f32 {
        if self.config.audio.replaygain {
            crate::eq::gain_factor(track.gain_db)
        } else {
            1.0
        }
    }

    fn apply_eq(&self) {
        self.engine.eq.set(&self.config.audio.eq_gains());
    }

    fn cycle_eq(&mut self) {
        let name = crate::eq::next_preset(&self.config.audio.eq);
        self.set_eq(name);
    }

    fn set_eq(&mut self, name: &str) {
        self.config.audio.eq = name.to_string();
        self.config.audio.eq_bands = None;
        self.apply_eq();
        match config::pick_eq(name) {
            Ok(()) => self.set_status(format!("Equalizer: {name}"), false),
            Err(error) => {
                self.set_status(format!("Equalizer: {name} (not saved: {error:#})"), true)
            }
        }
    }

    fn peek_next(&mut self) -> Option<(LocalTrack, Active, bool, Option<Resume>)> {
        let resume = match self.active {
            Active::Local(i) => self.tracks.get(i).map(|t| Resume::Local(t.path.clone())),
            Active::Queue(i) => Some(Resume::Queue(i)),
            _ => return None,
        };
        if let Some(track) = self.up_next.front().cloned() {
            return Some((track, Active::Extra, true, resume));
        }
        let (len, current, local) = match self.active {
            Active::Local(i) => (self.tracks.len(), i, true),
            Active::Queue(i) => (self.queue.len(), i, false),
            _ => return None,
        };
        if len == 0 {
            return None;
        }
        let index = if self.repeat == Repeat::One {
            current
        } else if self.shuffle {
            (self.next_random() % len as u64) as usize
        } else if current + 1 < len {
            current + 1
        } else if self.repeat == Repeat::All {
            0
        } else {
            return None;
        };
        if local {
            let track = self.tracks.get(index)?.clone();
            Some((track, Active::Local(index), false, None))
        } else {
            let track = self.queue.get(index)?.clone();
            Some((track, Active::Queue(index), false, None))
        }
    }

    fn commit_pick(&mut self, pick: Pick) {
        let active = pick.resolve_active(&self.tracks);
        if pick.pop {
            if self
                .up_next
                .front()
                .is_some_and(|t| t.path == pick.track.path)
            {
                self.up_next.pop_front();
            }
            self.resume = pick.resume;
        }
        if active == Active::Extra && matches!(pick.active, Active::Local(_)) {
            self.resume = None;
        }
        let track = pick.track;
        self.active = active;
        self.duration = pick.duration;
        self.title = track.title.clone();
        self.artist = track.artist_or_folder();
        self.album = track.album.clone();
        self.current_path = Some(track.path.clone());
        self.load_cover(&track.path);
        match active {
            Active::Local(index) => self.library_state.select(Some(index)),
            Active::Queue(index) => {
                if let Some(open) = self.open_playlist
                    && self.queue_is_playlist
                    && self
                        .playlists
                        .get(open)
                        .is_some_and(|p| p.name == self.queue_name)
                {
                    self.playlist_track_state.select(Some(index));
                }
            }
            _ => {}
        }
        self.set_status("", false);
    }

    fn drive_transitions(&mut self) {
        self.engine.update();
        if let Some(epoch) = self.preloaded.as_ref().map(|p| p.epoch) {
            if epoch != self.engine.epoch() || self.engine.is_finished() {
                self.preloaded = None;
            } else if self.engine.sources() <= 1
                && let Some(pick) = self.preloaded.take()
            {
                self.commit_pick(pick);
            }
            return;
        }
        if self.loading || !matches!(self.active, Active::Local(_) | Active::Queue(_)) {
            return;
        }
        let Some(total) = self.duration else {
            return;
        };
        if self.engine.is_paused() || self.preload_skip == self.current_path {
            return;
        }
        let remaining = total.saturating_sub(self.engine.position());
        let fade = self.config.audio.fade();
        let crossfade = !fade.is_zero() && total > fade * 3;
        let window = if crossfade {
            fade
        } else if self.config.audio.gapless {
            Duration::from_secs(8)
        } else {
            return;
        };
        if remaining > window {
            return;
        }
        let Some((track, active, pop, resume)) = self.peek_next() else {
            self.preload_skip = self.current_path.clone();
            return;
        };
        let gain = self.gain_for(&track);
        let source =
            match Engine::open_file(&track.path, self.engine.tap(), self.engine.eq.clone(), gain) {
                Ok(source) => source,
                Err(_) => {
                    self.preload_skip = self.current_path.clone();
                    return;
                }
            };
        if crossfade {
            let duration = self.engine.crossfade_to(source, fade);
            self.commit_pick(Pick {
                track,
                active,
                pop,
                resume,
                duration,
                epoch: self.engine.epoch(),
            });
        } else {
            let duration = self.engine.queue_next(source);
            self.preloaded = Some(Pick {
                track,
                active,
                pop,
                resume,
                duration,
                epoch: self.engine.epoch(),
            });
        }
    }

    fn load_cover(&mut self, path: &std::path::Path) {
        self.lyrics = lyrics::load(path);
        if self.lyrics.is_none() {
            self.lyrics_view = None;
        }
        self.begin_listen();
        self.cover = None;
        self.cover_for = Some(path.to_path_buf());
        self.cover_serial += 1;
        let serial = self.cover_serial;
        let path = path.to_path_buf();
        let tx = self.cover_tx.clone();
        std::thread::spawn(move || {
            let bytes = library::read_cover(&path);
            let cover = bytes.as_deref().and_then(cover::decode);
            let file = bytes.and_then(|bytes| {
                let dir = dirs::cache_dir()?.join("crab");
                std::fs::create_dir_all(&dir).ok()?;
                let target = dir.join(format!("cover-{}-{serial}", std::process::id()));
                std::fs::write(&target, bytes).ok()?;
                Some(target)
            });
            let _ = tx.send((path, cover, file));
        });
    }

    pub fn play_local(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index).cloned() else {
            return;
        };
        if self.open_track(&track, Active::Local(index)) {
            self.library_state.select(Some(index));
        }
    }

    fn play_queue(&mut self, index: usize) -> bool {
        let Some(track) = self.queue.get(index).cloned() else {
            return false;
        };
        if !self.open_track(&track, Active::Queue(index)) {
            return false;
        }
        if let Some(open) = self.open_playlist
            && self.queue_is_playlist
            && self
                .playlists
                .get(open)
                .is_some_and(|p| p.name == self.queue_name)
        {
            self.playlist_track_state.select(Some(index));
        }
        true
    }

    fn play_playlist(&mut self, list: usize, index: usize) {
        let Some(playlist) = self.playlists.get(list) else {
            return;
        };
        let name = playlist.name.clone();
        let paths = playlist.tracks.clone();
        self.queue = paths.iter().map(|p| self.resolve_track(p)).collect();
        self.queue_name = name;
        self.queue_is_playlist = true;
        self.play_queue(index);
    }

    fn advance_queue(&mut self, current: usize) {
        let len = self.queue.len();
        if self.repeat == Repeat::One && current < len && self.play_queue(current) {
            return;
        }
        let mut index = current;
        for _ in 0..len {
            if self.shuffle {
                index = (self.next_random() % len as u64) as usize;
            } else if index + 1 < len {
                index += 1;
            } else if self.repeat == Repeat::All {
                index = 0;
            } else {
                break;
            }
            if self.play_queue(index) {
                return;
            }
        }
        self.engine.stop();
        self.active = Active::None;
    }

    pub fn play_radio(&mut self, index: usize) {
        let Some(station) = self.stations.get(index).cloned() else {
            return;
        };
        self.pause_spotify();
        self.generation += 1;
        let generation = self.generation;
        self.engine.stop();
        self.active = Active::Radio(index);
        self.loading = true;
        self.duration = None;
        self.title = station.name.clone();
        self.artist = String::from("Internet radio");
        self.radio_title = Arc::new(Mutex::new(String::new()));
        self.radio_state.select(Some(index));
        self.set_status(format!("Connecting to {}", station.name), false);

        let title = self.radio_title.clone();
        let tap = self.engine.tap();
        let eq = self.engine.eq.clone();
        let tx = self.radio_tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Track> {
                let connection = radio::connect(&station.url, title)?;
                Engine::open_stream(connection.stream, &connection.mime, tap, eq)
            })();
            let _ = tx.send((generation, result.map_err(|e| format!("{e:#}"))));
        });
    }

    fn advance_local(&mut self, current: usize) {
        let len = self.tracks.len();
        if len == 0 {
            return;
        }
        if self.repeat == Repeat::One && current < len {
            self.play_local(current);
            return;
        }
        let next = if self.shuffle {
            Some((self.next_random() % len as u64) as usize)
        } else if current + 1 < len {
            Some(current + 1)
        } else if self.repeat == Repeat::All {
            Some(0)
        } else {
            None
        };
        match next {
            Some(index) => self.play_local(index),
            None => {
                self.engine.stop();
                self.active = Active::None;
            }
        }
    }

    fn start_queued(&mut self) -> bool {
        if self.up_next.is_empty() {
            return false;
        }
        let resume = match self.active {
            Active::Local(i) => self.tracks.get(i).map(|t| Resume::Local(t.path.clone())),
            Active::Queue(i) => Some(Resume::Queue(i)),
            Active::Extra => self.resume.take(),
            _ => None,
        };
        while let Some(track) = self.up_next.pop_front() {
            if self.open_track(&track, Active::Extra) {
                self.resume = resume;
                return true;
            }
        }
        false
    }

    fn resume_after(&mut self) {
        match self.resume.take() {
            Some(Resume::Local(path)) => match self.tracks.iter().position(|t| t.path == path) {
                Some(index) => self.advance_local(index),
                None => {
                    self.engine.stop();
                    self.active = Active::None;
                }
            },
            Some(Resume::Queue(index)) => self.advance_queue(index),
            None => {
                self.engine.stop();
                self.active = Active::None;
            }
        }
    }

    fn step(&mut self, forward: bool) {
        if forward
            && matches!(self.active, Active::Local(_) | Active::Queue(_))
            && self.start_queued()
        {
            return;
        }
        match self.active {
            Active::Spotify => {
                if let Some(handle) = &self.spotify {
                    handle.send(if forward {
                        SpotifyCommand::Next
                    } else {
                        SpotifyCommand::Previous
                    });
                }
            }
            Active::Extra => {
                if forward {
                    if !self.start_queued() {
                        self.resume_after();
                    }
                } else {
                    self.engine.seek_by(false, Duration::from_secs(3600), None);
                }
            }
            Active::Local(current) => {
                let len = self.tracks.len();
                if len == 0 {
                    return;
                }
                let index = if self.shuffle {
                    (self.next_random() % len as u64) as usize
                } else if forward {
                    (current + 1) % len
                } else {
                    (current + len - 1) % len
                };
                self.play_local(index);
            }
            Active::Queue(current) => {
                let len = self.queue.len();
                if len == 0 {
                    return;
                }
                let index = if self.shuffle {
                    (self.next_random() % len as u64) as usize
                } else if forward {
                    (current + 1) % len
                } else {
                    (current + len - 1) % len
                };
                self.play_queue(index);
            }
            Active::Radio(current) => {
                let len = self.stations.len();
                if len == 0 {
                    return;
                }
                let index = if forward {
                    (current + 1) % len
                } else {
                    (current + len - 1) % len
                };
                self.play_radio(index);
            }
            Active::None => self.play_selected(),
        }
    }

    fn play_selected(&mut self) {
        match self.tab {
            Tab::Library => {
                if !self.browser.is_songs() {
                    self.play_browser(true);
                    return;
                }
                if let Some(index) = self.library_state.selected() {
                    self.play_local(index);
                }
            }
            Tab::Playlists => match self.open_playlist {
                None => {
                    if let Some(index) = self.playlist_state.selected()
                        && let Some(list) = self.playlists.get(index)
                    {
                        if list.tracks.is_empty() {
                            self.set_status("This playlist is empty, press e to open it", true);
                        } else {
                            self.play_playlist(index, 0);
                        }
                    }
                }
                Some(list) => {
                    if let Some(index) = self.playlist_track_state.selected() {
                        self.play_playlist(list, index);
                    }
                }
            },
            Tab::Radio => {
                if let Some(index) = self.radio_state.selected() {
                    self.play_radio(index);
                }
            }
            Tab::Spotify => {
                if let Some(handle) = &self.spotify {
                    handle.send(SpotifyCommand::PlayPause);
                }
            }
        }
    }

    fn toggle_pause(&mut self) {
        match self.active {
            Active::Spotify => {
                if let Some(handle) = &self.spotify {
                    handle.send(SpotifyCommand::PlayPause);
                }
            }
            Active::Local(_) | Active::Queue(_) | Active::Extra | Active::Radio(_) => {
                if !self.loading {
                    self.engine.toggle_pause();
                }
            }
            Active::None => self.play_selected(),
        }
    }

    fn library_len(&self) -> usize {
        if self.tab == Tab::Library && !self.browser.is_songs() {
            return self.browser.rows.len();
        }
        match (self.tab, self.open_playlist) {
            (Tab::Playlists, None) => self.playlists.len(),
            (Tab::Playlists, Some(list)) => self.playlists.get(list).map_or(0, |p| p.tracks.len()),
            _ => self.tracks.len(),
        }
    }

    fn library_state_mut(&mut self) -> &mut ListState {
        if self.tab == Tab::Library && !self.browser.is_songs() {
            return &mut self.browser.state;
        }
        match (self.tab, self.open_playlist) {
            (Tab::Playlists, None) => &mut self.playlist_state,
            (Tab::Playlists, Some(_)) => &mut self.playlist_track_state,
            _ => &mut self.library_state,
        }
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Library | Tab::Playlists => self.library_len(),
            Tab::Radio => self.stations.len(),
            Tab::Spotify => 0,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let state = match self.tab {
            Tab::Library | Tab::Playlists => self.library_state_mut(),
            Tab::Radio => &mut self.radio_state,
            Tab::Spotify => return,
        };
        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        state.select(Some(next));
    }

    fn switch_tab(&mut self, forward: bool) {
        let index = self.tab.index();
        let count = Tab::ALL.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.tab = Tab::ALL[next];
    }

    pub fn roots_label(&self) -> String {
        let label = |path: &PathBuf| -> String {
            match dirs::home_dir() {
                Some(home) => match path.strip_prefix(&home) {
                    Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                    Ok(rest) => format!("~/{}", rest.display()),
                    Err(_) => path.display().to_string(),
                },
                None => path.display().to_string(),
            }
        };
        match self.roots.as_slice() {
            [] => "no folder".to_string(),
            [only] => label(only),
            [first, rest @ ..] => format!("{} +{}", label(first), rest.len()),
        }
    }

    fn open_picker(&mut self) {
        let start = self
            .roots
            .first()
            .cloned()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        self.picker = Some(Picker::new(start));
    }

    fn picker_key(&mut self, key: KeyEvent) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match picker.handle_key(key) {
            Outcome::Stay => {}
            Outcome::Collect(paths) => self.collect_into_playlist(paths),
            Outcome::Cancel => self.picker = None,
            Outcome::Replace(path) => {
                self.picker = None;
                self.change_library(vec![path], true);
            }
            Outcome::Add(path) => {
                self.picker = None;
                let mut roots = self.roots.clone();
                if !roots.contains(&path) {
                    roots.push(path);
                }
                self.change_library(roots, false);
            }
        }
    }

    fn change_library(&mut self, roots: Vec<PathBuf>, replace: bool) {
        if replace {
            if let Active::Local(_) = self.active {
                self.engine.stop();
                self.active = Active::None;
            }
            self.tracks.clear();
            self.all_tracks.clear();
            self.library_state.select(None);
            self.set_library_view(self.browser.view);
        }
        self.roots = roots;
        self.watcher = library::watch(&self.roots);
        self.start_scan();
        self.rescan_at = None;
        self.scanning = true;
        self.tab = Tab::Library;
        library::save_roots(&self.roots);
        let label = self.roots_label();
        self.set_status(format!("Library: {label}"), false);
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.help.is_some() {
            self.help_key(key);
            return;
        }
        if self.lyrics_view.is_some() {
            self.lyrics_key(key);
            return;
        }
        if self.queue_view.is_some() {
            self.queue_view_key(key);
            return;
        }
        if self.name_prompt.is_some() {
            self.name_prompt_key(key);
            return;
        }
        if self.cmdline.is_some() {
            self.cmdline_key(key);
            return;
        }
        if self.filtering {
            self.filter_key(key);
            return;
        }
        if self.finder.is_some() {
            self.finder_key(key);
            return;
        }
        if self.theme_editor.is_some() {
            self.theme_editor_key(key);
            return;
        }
        if self.theme_generator.is_some() {
            self.theme_generator_key(key);
            return;
        }
        if self.theme_picker.is_some() {
            self.theme_picker_key(key);
            return;
        }
        if self.picker.is_some() {
            self.picker_key(key);
            return;
        }
        self.normal_key(key);
    }

    fn normal_key(&mut self, key: KeyEvent) {
        let parsed = Key::from_event(&key);
        if let Some(k) = parsed
            && self.keys.vim()
            && let Some(digit) = k.is_digit()
            && (self.count.is_some() || (digit != 0 && !self.keys.is_bound(&k)))
        {
            let n = self
                .count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit)
                .min(9999);
            self.count = Some(n);
            self.pending_g = false;
            self.set_status(n.to_string(), false);
            return;
        }
        if let Some(k) = parsed
            && self.keys.vim()
            && k.is_char('g')
            && !self.keys.is_bound(&k)
        {
            if self.pending_g {
                self.pending_g = false;
                let count = self.count.take();
                self.go_to(count, false);
            } else {
                self.pending_g = true;
            }
            return;
        }
        self.pending_g = false;
        let count = self.count.take();
        let Some(action) = self.keys.action(&key) else {
            return;
        };
        if action != Action::Remove {
            self.pending_remove = None;
        }
        if self.visual.is_some() && !action.is_motion() {
            self.visual = None;
        }
        self.run_action(action, count);
        if self.visual.is_some() {
            self.update_visual();
        }
    }

    fn go_to(&mut self, count: Option<usize>, bottom: bool) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let index = match count {
            Some(n) => n.saturating_sub(1).min(len - 1),
            None if bottom => len - 1,
            None => 0,
        };
        match self.tab {
            Tab::Library | Tab::Playlists => self.library_state_mut().select(Some(index)),
            Tab::Radio => self.radio_state.select(Some(index)),
            Tab::Spotify => {}
        }
    }

    fn half_page(&self) -> isize {
        ((self.hit.body.height / 2).max(1)) as isize
    }

    fn run_action(&mut self, action: Action, count: Option<usize>) {
        let n = count.unwrap_or(1).max(1);
        match action {
            Action::Quit => self.quit = true,
            Action::Cancel => {
                if self.visual.take().is_none() && !self.cancel_loading() && !self.clear_filter() {
                    if !self.library_back() {
                        self.close_playlist();
                    }
                }
            }
            Action::Back => {
                if !self.library_back() {
                    self.close_playlist();
                }
            }
            Action::LibraryView if self.tab == Tab::Library => self.cycle_library_view(),
            Action::PlayGroup if self.tab == Tab::Library => self.play_browser(false),
            Action::Search => match self.tab {
                Tab::Library => self.filtering = true,
                Tab::Radio => self.open_finder(),
                _ => {}
            },
            Action::Repeat => {
                self.repeat = self.repeat.next();
                self.set_status(format!("Repeat {}", self.repeat.label()), false);
            }
            Action::Queue => self.queue_add(),
            Action::QueueView => {
                let mut state = ListState::default();
                if !self.up_next.is_empty() {
                    state.select(Some(0));
                }
                self.queue_view = Some(state);
            }
            Action::Help => self.help = Some(0),
            Action::Mark => {
                if self.in_folders() {
                    self.toggle_mark();
                } else if self.in_playlists() {
                    self.add_to_playlist();
                }
            }
            Action::MarkAll if self.in_folders() => self.toggle_mark_all(),
            Action::Sort if self.in_folders() => self.next_sort(),
            Action::TrackMark if self.in_open_playlist() => self.toggle_track_mark(),
            Action::TrackMarkAll if self.in_open_playlist() => self.toggle_track_mark_all(),
            Action::Save if self.in_folders() => self.save_marked(),
            Action::Remove => {
                if self.in_playlists() {
                    self.remove_from_playlists();
                } else if self.tab == Tab::Radio {
                    self.remove_station();
                }
            }
            Action::OpenList if self.in_playlists() => self.open_selected_playlist(),
            Action::Import if self.in_playlists() => self.open_import_prompt(),
            Action::Export if self.in_playlists() => self.open_export_prompt(),
            Action::Sleep => self.cycle_sleep(),
            Action::Eq => self.cycle_eq(),
            Action::Lyrics => self.toggle_lyrics(),
            Action::Visualizer => self.next_visualizer(),
            Action::Clock => self.toggle_clock(),
            Action::Folder => self.open_picker(),
            Action::TabNext => self.switch_tab(true),
            Action::TabPrev => self.switch_tab(false),
            Action::Tab1 => self.tab = Tab::Library,
            Action::Tab2 => self.tab = Tab::Playlists,
            Action::Tab3 => self.tab = Tab::Radio,
            Action::Tab4 => self.tab = Tab::Spotify,
            Action::Up => self.move_selection(-(n as isize)),
            Action::Down => self.move_selection(n as isize),
            Action::PageUp => self.move_selection(-10 * n as isize),
            Action::PageDown => self.move_selection(10 * n as isize),
            Action::HalfUp => self.move_selection(-self.half_page() * n as isize),
            Action::HalfDown => self.move_selection(self.half_page() * n as isize),
            Action::Top => self.go_to(count, false),
            Action::Bottom => self.go_to(count, true),
            Action::Play => self.play_selected(),
            Action::Pause => self.toggle_pause(),
            Action::Next => (0..n).for_each(|_| self.step(true)),
            Action::Prev => (0..n).for_each(|_| self.step(false)),
            Action::SeekForward => (0..n).for_each(|_| self.seek(true)),
            Action::SeekBack => (0..n).for_each(|_| self.seek(false)),
            Action::VolumeUp => self.change_volume(0.05 * n as f32),
            Action::VolumeDown => self.change_volume(-0.05 * n as f32),
            Action::Mute => self.toggle_mute(),
            Action::Theme => self.open_theme_picker(),
            Action::Reload => self.reload_config(),
            Action::Shuffle => self.set_shuffle(!self.shuffle),
            Action::Visual => self.toggle_visual(),
            Action::Command => self.cmdline = Some(String::new()),
            Action::Stats => self.toggle_stats(),
            Action::Favorite => self.toggle_favorite(),
            Action::Favorites => self.set_only_favorites(!self.only_favorites),
            _ => {}
        }
    }

    fn toggle_stats(&mut self) {
        if self.stats.take().is_none() {
            let mut stats = Stats::new();
            stats.refresh();
            self.stats = Some(stats);
        }
    }

    pub fn stats_badge(&self) -> Option<String> {
        self.stats.as_ref().map(Stats::badge)
    }

    pub fn visual_active(&self) -> bool {
        self.visual.is_some()
    }

    fn toggle_visual(&mut self) {
        if self.visual.take().is_some() {
            return;
        }
        let anchor = if self.in_folders() {
            self.library_state.selected()
        } else if self.in_open_playlist() {
            self.playlist_track_state.selected()
        } else {
            None
        };
        let Some(anchor) = anchor else {
            self.set_status(
                "Visual selection works in the Library and in open playlists",
                false,
            );
            return;
        };
        self.visual = Some(Visual {
            anchor,
            base: self.marked.clone(),
            base_index: self.marked_tracks.clone(),
        });
        self.update_visual();
    }

    fn update_visual(&mut self) {
        let Some(visual) = self.visual.as_ref() else {
            return;
        };
        let current = if self.tab == Tab::Library {
            self.library_state.selected()
        } else {
            self.playlist_track_state.selected()
        };
        let Some(current) = current else {
            return;
        };
        let (lo, hi) = (visual.anchor.min(current), visual.anchor.max(current));
        if self.tab == Tab::Library {
            let mut marked = visual.base.clone();
            marked.extend(
                self.tracks
                    .iter()
                    .skip(lo)
                    .take(hi + 1 - lo)
                    .map(|t| t.path.clone()),
            );
            self.marked = marked;
        } else {
            let mut marked = visual.base_index.clone();
            marked.extend(lo..=hi);
            self.marked_tracks = marked;
        }
    }

    fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
        self.set_status(if on { "Shuffle on" } else { "Shuffle off" }, false);
    }

    fn cmdline_key(&mut self, key: KeyEvent) {
        let Some(mut input) = self.cmdline.take() else {
            return;
        };
        if key.code != KeyCode::Tab {
            self.cmd_cycle = None;
        }
        match key.code {
            KeyCode::Esc => {
                self.cmd_pos = None;
                return;
            }
            KeyCode::Enter => {
                self.cmd_pos = None;
                if !input.trim().is_empty() && self.cmd_history.last() != Some(&input) {
                    self.cmd_history.push(input.clone());
                    if self.cmd_history.len() > 50 {
                        self.cmd_history.remove(0);
                    }
                }
                self.run_command(&input);
                return;
            }
            KeyCode::Backspace => {
                if input.pop().is_none() {
                    return;
                }
                self.cmd_pos = None;
            }
            KeyCode::Tab => self.complete_command(&mut input),
            KeyCode::Up if !self.cmd_history.is_empty() => {
                let pos = match self.cmd_pos {
                    None => {
                        self.cmd_draft = input.clone();
                        self.cmd_history.len() - 1
                    }
                    Some(pos) => pos.saturating_sub(1),
                };
                self.cmd_pos = Some(pos);
                input = self.cmd_history[pos].clone();
            }
            KeyCode::Down => {
                if let Some(pos) = self.cmd_pos {
                    if pos + 1 < self.cmd_history.len() {
                        self.cmd_pos = Some(pos + 1);
                        input = self.cmd_history[pos + 1].clone();
                    } else {
                        self.cmd_pos = None;
                        input = std::mem::take(&mut self.cmd_draft);
                    }
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.push(c);
                self.cmd_pos = None;
            }
            _ => {}
        }
        self.cmdline = Some(input);
    }

    fn theme_names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }

    fn complete_command(&mut self, input: &mut String) {
        if let Some(cycle) = self.cmd_cycle.as_mut()
            && cycle.list.len() > 1
            && *input == format!("{}{}", cycle.base, cycle.list[cycle.index])
        {
            cycle.index = (cycle.index + 1) % cycle.list.len();
            *input = format!("{}{}", cycle.base, cycle.list[cycle.index]);
            return;
        }
        let (base, list) = commands::candidates(input, &self.theme_names());
        match list.len() {
            0 => {}
            1 => *input = format!("{base}{} ", list[0]),
            _ => {
                let common = commands::common_prefix(&list);
                if common.len() > input.len() - base.len() {
                    *input = format!("{base}{common}");
                } else {
                    *input = format!("{base}{}", list[0]);
                    self.cmd_cycle = Some(CmdCycle {
                        base,
                        list,
                        index: 0,
                    });
                }
            }
        }
    }

    pub fn cmd_hints(&self) -> (Vec<String>, Option<usize>) {
        let Some(input) = self.cmdline.as_ref() else {
            return (Vec::new(), None);
        };
        if input.trim().is_empty() {
            return (Vec::new(), None);
        }
        if let Some(cycle) = &self.cmd_cycle
            && input.starts_with(&cycle.base)
        {
            return (cycle.list.clone(), Some(cycle.index));
        }
        let (_, list) = commands::candidates(input, &self.theme_names());
        if list.len() == 1 && input.ends_with(list[0].as_str()) {
            return (Vec::new(), None);
        }
        (list, None)
    }

    fn find_track(&self, words: &[String]) -> Option<usize> {
        let words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        self.tracks
            .iter()
            .position(|t| Self::matches_filter(t, &words))
    }

    fn set_only_favorites(&mut self, on: bool) {
        self.only_favorites = on;
        self.tab = Tab::Library;
        self.rebuild_view();
        self.set_status(
            if on {
                format!("Showing {} favorites", self.tracks.len())
            } else {
                "Showing all tracks".to_string()
            },
            false,
        );
    }

    fn toggle_favorite(&mut self) {
        let paths: Vec<PathBuf> = if self.tab == Tab::Library && !self.browser.is_songs() {
            self.browser
                .selected()
                .into_iter()
                .flat_map(|row| row.tracks.iter().map(|track| track.path.clone()))
                .collect()
        } else if self.tab == Tab::Library {
            if self.marked.is_empty() {
                self.library_state
                    .selected()
                    .and_then(|i| self.tracks.get(i))
                    .map(|t| t.path.clone())
                    .into_iter()
                    .collect()
            } else {
                self.tracks
                    .iter()
                    .filter(|t| self.marked.contains(&t.path))
                    .map(|t| t.path.clone())
                    .collect()
            }
        } else if matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        ) {
            self.current_path.clone().into_iter().collect()
        } else {
            Vec::new()
        };
        if paths.is_empty() {
            self.set_status("Select or play a local track first", true);
            return;
        }
        let remove = paths.iter().all(|p| self.favorites.contains(p));
        for path in &paths {
            if remove {
                self.favorites.remove(path);
            } else {
                self.favorites.insert(path.clone());
            }
        }
        if self.tab == Tab::Library {
            self.marked.clear();
        }
        let count = paths.len();
        let noun = if count == 1 { "track" } else { "tracks" };
        self.set_status(
            if remove {
                format!("Removed {count} {noun} from favorites")
            } else {
                format!("Added {count} {noun} to favorites")
            },
            false,
        );
        if remove && self.only_favorites {
            self.rebuild_view();
        }
        state::save(&self.snapshot_state());
    }

    fn run_command(&mut self, line: &str) {
        let (name, args) = commands::split(line);
        if name.is_empty() {
            return;
        }
        let Some(command) = commands::canonical(&name) else {
            self.set_status(format!("Unknown command: {name}"), true);
            return;
        };
        let arg = args.first().map(String::as_str);
        match command {
            "quit" => self.quit = true,
            "help" => self.help = Some(0),
            "sort" => match arg {
                None => self.next_sort(),
                Some(text) => match library::SortKey::from_name(text) {
                    Some(sort) => self.set_sort(sort),
                    None => self.set_status("Sort by path, title, artist or album", true),
                },
            },
            "view" => match arg.and_then(View::from_name) {
                Some(view) => {
                    self.tab = Tab::Library;
                    self.set_library_view(view);
                }
                None => self.set_status("Usage: view songs|artists|albums", true),
            },
            "sleep" => match arg {
                None => self.cycle_sleep(),
                Some("off") | Some("0") => self.set_sleep(0),
                Some(text) => match text.parse::<u64>() {
                    Ok(minutes) if minutes <= 600 => self.set_sleep(minutes),
                    _ => self.set_status("Usage: sleep <minutes>|off", true),
                },
            },
            "eq" => match arg {
                None => self.cycle_eq(),
                Some(text) => match crate::eq::PRESETS
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(text))
                {
                    Some((preset, _)) => self.set_eq(preset),
                    None => {
                        let names: Vec<&str> = crate::eq::PRESETS.iter().map(|(n, _)| *n).collect();
                        self.set_status(format!("Presets: {}", names.join(", ")), true);
                    }
                },
            },
            "repeat" => match arg {
                None => {
                    self.repeat = self.repeat.next();
                    self.set_status(format!("Repeat {}", self.repeat.label()), false);
                }
                Some(text @ ("off" | "all" | "one")) => {
                    self.repeat = Repeat::from_name(text);
                    self.set_status(format!("Repeat {}", self.repeat.label()), false);
                }
                Some(_) => self.set_status("Usage: repeat off|all|one", true),
            },
            "shuffle" => match arg {
                None => self.set_shuffle(!self.shuffle),
                Some("on") => self.set_shuffle(true),
                Some("off") => self.set_shuffle(false),
                Some(_) => self.set_status("Usage: shuffle on|off", true),
            },
            "volume" => match arg.and_then(|a| a.trim_end_matches('%').parse::<u32>().ok()) {
                Some(percent) if percent <= 100 => self.set_volume_to(percent as f32 / 100.0),
                _ => self.set_status("Usage: volume 0-100", true),
            },
            "lyrics" => self.toggle_lyrics(),
            "theme" => match arg {
                None => self.open_theme_picker(),
                Some(_) => self.set_theme(&args.join(" ")),
            },
            "viz" => match arg {
                None => self.next_visualizer(),
                Some(text) => match config::VizStyle::from_name(text) {
                    Some(style) => self.set_visualizer(style),
                    None => {
                        let names: Vec<&str> =
                            config::VizStyle::ALL.iter().map(|s| s.name()).collect();
                        self.set_status(format!("Styles: {}", names.join(", ")), true);
                    }
                },
            },
            "clock" => self.toggle_clock(),
            "reload" => self.reload_config(),
            "folder" => self.open_picker(),
            "import" => {
                self.tab = Tab::Playlists;
                self.open_import_prompt();
            }
            "export" => {
                self.tab = Tab::Playlists;
                self.open_export_prompt();
            }
            "save" => {
                self.tab = Tab::Library;
                if !self.browser.is_songs() {
                    self.set_library_view(View::Songs);
                }
                self.save_marked();
            }
            "tab" => {
                let tab = match arg.map(str::to_ascii_lowercase).as_deref() {
                    Some("1" | "library") => Some(Tab::Library),
                    Some("2" | "playlists") => Some(Tab::Playlists),
                    Some("3" | "radio") => Some(Tab::Radio),
                    Some("4" | "spotify") => Some(Tab::Spotify),
                    _ => None,
                };
                match tab {
                    Some(tab) => self.tab = tab,
                    None => self.set_status("Usage: tab library|playlists|radio|spotify", true),
                }
            }
            "play" if !args.is_empty() => match self.find_track(&args) {
                Some(index) => {
                    self.tab = Tab::Library;
                    self.set_library_view(View::Songs);
                    self.library_state.select(Some(index));
                    self.play_local(index);
                }
                None => self.set_status("No track matches", true),
            },
            "play" => {
                if !self.is_playing() {
                    self.toggle_pause();
                }
            }
            "goto" => match self.find_track(&args) {
                Some(index) if !args.is_empty() => {
                    self.tab = Tab::Library;
                    self.set_library_view(View::Songs);
                    self.library_state.select(Some(index));
                }
                _ => self.set_status("Usage: goto <text> (no track matches)", true),
            },
            "find" => {
                self.tab = Tab::Library;
                self.filter = args.join(" ");
                self.rebuild_view();
                let text = if self.filter.is_empty() {
                    "Filter cleared".to_string()
                } else {
                    format!("{} matches", self.tracks.len())
                };
                self.set_status(text, false);
            }
            "add" => {
                let words: Vec<String> = args.iter().map(|w| w.to_lowercase()).collect();
                if words.is_empty() {
                    self.set_status("Usage: add <text>", true);
                } else {
                    let found: Vec<LocalTrack> = self
                        .all_tracks
                        .iter()
                        .filter(|t| Self::matches_filter(t, &words))
                        .cloned()
                        .collect();
                    if found.is_empty() {
                        self.set_status("No track matches", true);
                    } else {
                        let count = found.len();
                        self.up_next.extend(found);
                        self.set_status(
                            format!("Queued {count} ({} up next)", self.up_next.len()),
                            false,
                        );
                    }
                }
            }
            "mute" => self.toggle_mute(),
            "fade" => match arg {
                None => {
                    let secs = self.config.audio.crossfade;
                    self.set_status(format!("Crossfade {secs} s"), false);
                }
                Some("off") => {
                    self.config.audio.crossfade = 0.0;
                    self.set_status("Crossfade off", false);
                }
                Some(text) => match text.trim_end_matches('s').parse::<f32>() {
                    Ok(secs) if (0.0..=12.0).contains(&secs) => {
                        self.config.audio.crossfade = secs;
                        self.set_status(format!("Crossfade {secs} s"), false);
                    }
                    _ => self.set_status("Usage: fade 0-12|off", true),
                },
            },
            "fav" => self.toggle_favorite(),
            "favs" => match arg {
                Some("on") => self.set_only_favorites(true),
                Some("off") => self.set_only_favorites(false),
                None => self.set_only_favorites(!self.only_favorites),
                Some(_) => self.set_status("Usage: favs on|off", true),
            },
            "pause" => {
                if self.is_playing() {
                    self.toggle_pause();
                }
            }
            "next" => self.step(true),
            "prev" => self.step(false),
            "stop" => self.handle_media(crate::media::Command::Stop),
            "seek" => match arg.and_then(commands::parse_seek) {
                Some(SeekTo::Absolute(position)) => {
                    self.handle_media(crate::media::Command::SetPosition(position))
                }
                Some(SeekTo::Relative(secs)) => {
                    self.handle_media(crate::media::Command::SeekBy(secs))
                }
                None => self.set_status("Usage: seek 1:30 | +10 | -10", true),
            },
            "queue" => self.queue_add(),
            "stats" => {
                if self.stats.is_none() {
                    self.toggle_stats();
                }
                let detail = self.stats.as_ref().map(Stats::detail).unwrap_or_default();
                self.set_status(detail, false);
            }
            "keys" => self.keys_command(arg),
            "reset" => match arg {
                Some(action) => self.reset_key(action),
                None => self.set_status("Usage: reset <action>", true),
            },
            "map" => self.map_command(&args),
            "unmap" => match args.first() {
                Some(action) => self.bind_keys(action, Vec::new()),
                None => self.set_status("Usage: unmap <action>", true),
            },
            _ => {}
        }
    }

    fn map_command(&mut self, args: &[String]) {
        match args {
            [] => self.set_status("Usage: map <action> <key>...", true),
            [name] => match keys::action_by_name(name) {
                Some(action) => {
                    let text = format!("{}: {}", keys::action_name(action), self.keys.all(action));
                    self.set_status(text, false);
                }
                None => self.set_status(format!("Unknown action: {name}"), true),
            },
            [name, rest @ ..] => self.bind_keys(name, rest.to_vec()),
        }
    }

    fn bind_keys(&mut self, name: &str, keys_list: Vec<String>) {
        let Some(action) = keys::action_by_name(name) else {
            self.set_status(format!("Unknown action: {name}"), true);
            return;
        };
        let action_name = keys::action_name(action);
        let mut stolen = Vec::new();
        for text in &keys_list {
            match Key::parse(text) {
                Some(key) => {
                    if let Some(other) = self.keys.lookup(&key)
                        && other != action
                    {
                        stolen.push(keys::action_name(other));
                    }
                }
                None => {
                    self.set_status(format!("Invalid key: {text}"), true);
                    return;
                }
            }
        }
        let base = if self.config.keys.preset == "custom" {
            self.config.keys.base.clone()
        } else if self.config.keys.preset == "vim" {
            "vim".to_string()
        } else {
            "crab".to_string()
        };
        if let Err(error) = config::pick_custom_key(action_name, &keys_list, &base) {
            self.set_status(format!("Not saved: {error:#}"), true);
            return;
        }
        self.config.keys.preset = "custom".to_string();
        self.config.keys.base = base.clone();
        self.config.keys.custom.insert(
            action_name.to_string(),
            config::KeyList::Many(keys_list.clone()),
        );
        let (map, _) = self.config.keymap();
        self.keys = map;
        let mut text = if keys_list.is_empty() {
            format!("{action_name} unbound (custom keys, base {base})")
        } else {
            format!(
                "{action_name}: {} (custom keys, base {base})",
                keys_list.join(" ")
            )
        };
        stolen.dedup();
        if !stolen.is_empty() {
            text.push_str(&format!(", taken from {}", stolen.join(", ")));
        }
        self.set_status(text, false);
    }

    fn keys_command(&mut self, arg: Option<&str>) {
        let Some(name) = arg else {
            let text = format!("Key preset: {}", self.config.keys.preset);
            self.set_status(text, false);
            return;
        };
        let Some(preset) = keys::Preset::from_name(name) else {
            self.set_status("Usage: keys crab|vim|custom", true);
            return;
        };
        if let Err(error) = config::pick_preset(preset.name()) {
            self.set_status(format!("Not saved: {error:#}"), true);
            return;
        }
        self.config.keys.preset = preset.name().to_string();
        let (map, warnings) = self.config.keymap();
        self.keys = map;
        match warnings.first() {
            Some(warning) => self.set_status(warning.clone(), true),
            None => self.set_status(format!("Key preset: {}", preset.name()), false),
        }
    }

    fn reset_key(&mut self, name: &str) {
        let Some(action) = keys::action_by_name(name) else {
            self.set_status(format!("Unknown action: {name}"), true);
            return;
        };
        let action_name = keys::action_name(action);
        if let Err(error) = config::reset_custom_key(action_name) {
            self.set_status(format!("Not saved: {error:#}"), true);
            return;
        }
        self.config.keys.custom.remove(action_name);
        let (map, _) = self.config.keymap();
        self.keys = map;
        self.set_status(format!("{action_name} back to the base keys"), false);
    }

    fn set_volume_to(&mut self, volume: f32) {
        match &self.sysvol {
            Some(sys) => {
                self.volume = volume.clamp(0.0, 1.0);
                sys.send(Request::Set(self.volume));
            }
            None => {
                self.engine.set_volume(volume);
                self.volume = self.engine.volume();
            }
        }
    }

    fn set_theme(&mut self, name: &str) {
        let Some(index) = self
            .themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        else {
            let names: Vec<&str> = self.themes.iter().map(|t| t.name.as_str()).collect();
            self.set_status(format!("Themes: {}", names.join(", ")), true);
            return;
        };
        self.theme_index = index;
        let name = self.theme().name.clone();
        match config::pick_theme(&name) {
            Ok(()) => {
                self.config.theme = name.clone();
                self.set_status(format!("Theme: {name}"), false);
            }
            Err(error) => self.set_status(format!("Theme {name} not saved: {error:#}"), true),
        }
    }

    fn cancel_loading(&mut self) -> bool {
        if self.loading && matches!(self.active, Active::Radio(_)) {
            self.generation += 1;
            self.engine.stop();
            self.active = Active::None;
            self.loading = false;
            self.set_status("Connection cancelled", false);
            return true;
        }
        false
    }

    fn in_folders(&self) -> bool {
        self.tab == Tab::Library && self.browser.is_songs()
    }

    fn in_playlists(&self) -> bool {
        self.tab == Tab::Playlists
    }

    fn open_selected_playlist(&mut self) {
        if self.open_playlist.is_some() {
            return;
        }
        if let Some(index) = self.playlist_state.selected()
            && index < self.playlists.len()
        {
            self.open_playlist = Some(index);
            self.marked_tracks.clear();
            self.playlist_track_state.select(Some(0));
        }
    }

    pub fn sleep_left(&self) -> Option<Duration> {
        self.sleep_at
            .map(|at| at.saturating_duration_since(Instant::now()))
    }

    fn cycle_sleep(&mut self) {
        let minutes = match self.sleep_left().map(|d| d.as_secs().div_ceil(60)) {
            None => 15,
            Some(m) if m <= 15 => 30,
            Some(m) if m <= 30 => 60,
            Some(m) if m <= 60 => 90,
            Some(_) => 0,
        };
        self.set_sleep(minutes);
    }

    fn set_sleep(&mut self, minutes: u64) {
        if minutes == 0 {
            self.sleep_at = None;
            self.set_status("Sleep timer off", false);
        } else {
            self.sleep_at = Some(Instant::now() + Duration::from_secs(minutes * 60));
            self.set_status(format!("Sleep timer: {minutes} minutes"), false);
        }
    }

    fn check_sleep(&mut self) {
        let Some(at) = self.sleep_at else {
            return;
        };
        if Instant::now() < at {
            return;
        }
        self.sleep_at = None;
        self.generation += 1;
        self.engine.stop();
        self.loading = false;
        self.pause_spotify();
        self.active = Active::None;
        self.set_status("Sleep timer: playback stopped", false);
    }

    fn overlay_open(&self) -> bool {
        self.finder.is_some()
            || self.theme_editor.is_some()
            || self.theme_generator.is_some()
            || self.theme_picker.is_some()
            || self.picker.is_some()
            || self.help.is_some()
            || self.lyrics_view.is_some()
            || self.queue_view.is_some()
            || self.name_prompt.is_some()
    }

    fn inside(rect: Rect, x: u16, y: u16) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    pub fn on_mouse(&mut self, event: MouseEvent) {
        if self.overlay_open() {
            return;
        }
        let (x, y) = (event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollUp => self.move_selection(-3),
            MouseEventKind::ScrollDown => self.move_selection(3),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, tab)) = self
                    .hit
                    .tabs
                    .iter()
                    .find(|(rect, _)| Self::inside(*rect, x, y))
                {
                    self.tab = *tab;
                    return;
                }
                if Self::inside(self.hit.progress, x, y) {
                    self.seek_to_ratio(x);
                    return;
                }
                if Self::inside(self.hit.body, x, y) {
                    self.click_row(y);
                }
            }
            _ => {}
        }
    }

    fn seek_to_ratio(&mut self, x: u16) {
        let progress = self.hit.progress;
        if progress.width == 0 {
            return;
        }
        let Some(total) = self.duration else {
            return;
        };
        if !matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        ) {
            return;
        }
        let ratio = (x.saturating_sub(progress.x)) as f64 / progress.width as f64;
        let target = total.mul_f64(ratio.clamp(0.0, 0.995));
        if !self.engine.seek_to(target) {
            self.set_status("Seeking is not supported for this file", true);
        }
    }

    fn click_row(&mut self, y: u16) {
        let body = self.hit.body;
        if y <= body.y || y + 1 >= body.y + body.height {
            return;
        }
        let row = (y - body.y - 1) as usize;
        let len = self.list_len();
        let state = match self.tab {
            Tab::Library | Tab::Playlists => self.library_state_mut(),
            Tab::Radio => &mut self.radio_state,
            Tab::Spotify => return,
        };
        let index = state.offset() + row;
        if index >= len {
            return;
        }
        state.select(Some(index));
        let double = self
            .last_click
            .is_some_and(|(at, last)| last == index && at.elapsed() < Duration::from_millis(450));
        if double {
            self.last_click = None;
            self.play_selected();
        } else {
            self.last_click = Some((Instant::now(), index));
        }
    }

    fn clear_filter(&mut self) -> bool {
        if self.tab != Tab::Library || self.filter.is_empty() {
            return false;
        }
        self.filter.clear();
        self.rebuild_view();
        true
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.rebuild_view();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                if self.filter.pop().is_none() {
                    self.filtering = false;
                }
                self.rebuild_view();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.rebuild_view();
            }
            _ => {}
        }
    }

    fn queue_add(&mut self) {
        let mut added: Vec<LocalTrack> = Vec::new();
        match self.tab {
            Tab::Library => {
                if !self.browser.is_songs() {
                    if let Some(row) = self.browser.selected() {
                        added = row.tracks.clone();
                    }
                } else {
                    let marked: Vec<LocalTrack> = self
                        .tracks
                        .iter()
                        .filter(|t| self.marked.contains(&t.path))
                        .cloned()
                        .collect();
                    if marked.is_empty() {
                        if let Some(track) = self
                            .library_state
                            .selected()
                            .and_then(|i| self.tracks.get(i))
                        {
                            added.push(track.clone());
                        }
                    } else {
                        added = marked;
                        self.marked.clear();
                    }
                }
            }
            Tab::Playlists => {
                let (list, indices): (Option<usize>, Vec<usize>) = match self.open_playlist {
                    Some(list) => {
                        let mut picked: Vec<usize> = self.marked_tracks.iter().copied().collect();
                        picked.sort_unstable();
                        if picked.is_empty() {
                            picked.extend(self.playlist_track_state.selected());
                        } else {
                            self.marked_tracks.clear();
                        }
                        (Some(list), picked)
                    }
                    None => {
                        let list = self.playlist_state.selected();
                        let count = list
                            .and_then(|i| self.playlists.get(i))
                            .map_or(0, |p| p.tracks.len());
                        (list, (0..count).collect())
                    }
                };
                if let Some(list) = list {
                    let paths: Vec<PathBuf> = indices
                        .iter()
                        .filter_map(|&i| self.playlists.get(list)?.tracks.get(i).cloned())
                        .collect();
                    for path in paths {
                        added.push(self.resolve_track(&path));
                    }
                }
            }
            _ => return,
        }
        if added.is_empty() {
            return;
        }
        let count = added.len();
        self.up_next.extend(added);
        self.set_status(
            format!("Queued {count} ({} up next)", self.up_next.len()),
            false,
        );
    }

    fn queue_view_key(&mut self, key: KeyEvent) {
        let len = self.up_next.len();
        let Some(state) = self.queue_view.as_mut() else {
            return;
        };
        let current = state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Esc | KeyCode::Char('Z') | KeyCode::Char('q') => self.queue_view = None,
            KeyCode::Up | KeyCode::Char('k') => state.select(Some(current.saturating_sub(1))),
            KeyCode::Down | KeyCode::Char('j') if len > 0 => {
                state.select(Some((current + 1).min(len - 1)));
            }
            KeyCode::Char('x') if current < len => {
                self.up_next.remove(current);
                let left = self.up_next.len();
                if let Some(state) = self.queue_view.as_mut() {
                    state.select(if left == 0 {
                        None
                    } else {
                        Some(current.min(left - 1))
                    });
                }
            }
            KeyCode::Char('X') => {
                self.up_next.clear();
                state.select(None);
            }
            KeyCode::Enter if current < len => {
                if let Some(track) = self.up_next.remove(current) {
                    self.up_next.push_front(track);
                    self.queue_view = None;
                    self.start_queued();
                }
            }
            _ => {}
        }
    }

    fn toggle_lyrics(&mut self) {
        if self.lyrics.is_some() {
            self.lyrics_view = Some(0);
        } else {
            self.set_status("No lyrics for this track", false);
        }
    }

    fn lyrics_key(&mut self, key: KeyEvent) {
        let Some(offset) = self.lyrics_view.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('L') | KeyCode::Char('q') => self.lyrics_view = None,
            KeyCode::Up | KeyCode::Char('k') => *offset = offset.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *offset += 1,
            KeyCode::PageUp => *offset = offset.saturating_sub(10),
            KeyCode::PageDown => *offset += 10,
            KeyCode::Home | KeyCode::Char('g') => *offset = 0,
            _ => {}
        }
    }

    fn help_key(&mut self, key: KeyEvent) {
        let Some(offset) = self.help.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.help = None,
            KeyCode::Up | KeyCode::Char('k') => *offset = offset.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *offset += 1,
            KeyCode::PageUp => *offset = offset.saturating_sub(10),
            KeyCode::PageDown => *offset += 10,
            KeyCode::Home | KeyCode::Char('g') => *offset = 0,
            _ => {}
        }
    }

    fn in_open_playlist(&self) -> bool {
        self.tab == Tab::Playlists && self.open_playlist.is_some()
    }

    fn toggle_mark_all(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        if self.tracks.iter().all(|t| self.marked.contains(&t.path)) {
            self.marked.clear();
        } else {
            self.marked = self.tracks.iter().map(|t| t.path.clone()).collect();
        }
    }

    fn toggle_track_mark(&mut self) {
        let Some(index) = self.playlist_track_state.selected() else {
            return;
        };
        if !self.marked_tracks.remove(&index) {
            self.marked_tracks.insert(index);
        }
        self.move_selection(1);
    }

    fn toggle_track_mark_all(&mut self) {
        let len = self.library_len();
        if len == 0 {
            return;
        }
        if self.marked_tracks.len() == len {
            self.marked_tracks.clear();
        } else {
            self.marked_tracks = (0..len).collect();
        }
    }

    fn close_playlist(&mut self) {
        if self.in_playlists() {
            self.open_playlist = None;
            self.marked_tracks.clear();
        }
    }

    fn toggle_mark(&mut self) {
        let Some(index) = self.library_state.selected() else {
            return;
        };
        let Some(track) = self.tracks.get(index) else {
            return;
        };
        let path = track.path.clone();
        if !self.marked.remove(&path) {
            self.marked.insert(path);
        }
        self.move_selection(1);
    }

    fn save_marked(&mut self) {
        let mut paths: Vec<PathBuf> = self
            .tracks
            .iter()
            .filter(|t| self.marked.contains(&t.path))
            .map(|t| t.path.clone())
            .collect();
        if paths.is_empty()
            && let Some(track) = self
                .library_state
                .selected()
                .and_then(|i| self.tracks.get(i))
        {
            paths.push(track.path.clone());
        }
        if paths.is_empty() {
            return;
        }
        self.name_prompt = Some(NamePrompt {
            title: "Save to playlist",
            label: "Name",
            input: String::new(),
            paths,
            kind: PromptKind::SaveMarked,
        });
    }

    fn add_to_playlist(&mut self) {
        match self.open_playlist {
            Some(list) => {
                if list >= self.playlists.len() {
                    return;
                }
                let start = self
                    .roots
                    .first()
                    .cloned()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| PathBuf::from("/"));
                self.picker = Some(Picker::for_files(start));
            }
            None => {
                self.name_prompt = Some(NamePrompt {
                    title: "New playlist",
                    label: "Name",
                    input: String::new(),
                    paths: Vec::new(),
                    kind: PromptKind::NewList,
                });
            }
        }
    }

    fn collect_into_playlist(&mut self, paths: Vec<PathBuf>) {
        let Some(list) = self.open_playlist else {
            return;
        };
        let Some(name) = self.playlists.get(list).map(|p| p.name.clone()) else {
            return;
        };
        let found: Vec<PathBuf> = paths.iter().flat_map(|p| library::collect(p)).collect();
        if found.is_empty() {
            if let Some(picker) = self.picker.as_mut() {
                picker.error = Some("No audio files there".to_string());
            }
            return;
        }
        for path in &found {
            self.resolve_track(path);
        }
        let (_, added) = playlist::add_tracks(&mut self.playlists, &name, found);
        let message = match playlist::save(&self.playlists) {
            Ok(()) => format!("Added {added} to {name}"),
            Err(error) => format!("Added {added} to {name} but not saved: {error:#}"),
        };
        if let Some(picker) = self.picker.as_mut() {
            picker.error = None;
        }
        self.set_status(message, false);
        if self.playlist_track_state.selected().is_none() {
            self.playlist_track_state.select(Some(0));
        }
    }

    fn name_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.name_prompt.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.name_prompt = None,
            KeyCode::Backspace => {
                prompt.input.pop();
            }
            KeyCode::Up | KeyCode::Down
                if !self.playlists.is_empty()
                    && matches!(prompt.kind, PromptKind::SaveMarked | PromptKind::NewList) =>
            {
                let current = playlist::find(&self.playlists, &prompt.input);
                let len = self.playlists.len();
                let next = match (current, key.code == KeyCode::Down) {
                    (Some(i), true) => (i + 1) % len,
                    (Some(i), false) => (i + len - 1) % len,
                    (None, true) => 0,
                    (None, false) => len - 1,
                };
                prompt.input = self.playlists[next].name.clone();
            }
            KeyCode::Char(c) => prompt.input.push(c),
            KeyCode::Enter => {
                let name = prompt.input.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let Some(prompt) = self.name_prompt.take() else {
                    return;
                };
                self.finish_prompt(prompt, name);
            }
            _ => {}
        }
    }

    fn finish_prompt(&mut self, prompt: NamePrompt, name: String) {
        match prompt.kind {
            PromptKind::ImportM3u => return self.import_m3u(&name),
            PromptKind::ExportM3u(index) => return self.export_m3u(index, &name),
            _ => {}
        }
        let new_list = prompt.kind == PromptKind::NewList;
        let (index, added) = playlist::add_tracks(&mut self.playlists, &name, prompt.paths);
        let name = self.playlists[index].name.clone();
        let saved = playlist::save(&self.playlists);
        if new_list {
            self.tab = Tab::Playlists;
            self.playlist_state.select(Some(index));
            self.open_playlist = Some(index);
            self.playlist_track_state
                .select(if self.playlists[index].tracks.is_empty() {
                    None
                } else {
                    Some(0)
                });
            self.add_to_playlist();
        } else {
            self.marked.clear();
            self.playlist_state.select(Some(index));
        }
        match saved {
            Ok(()) if new_list => self.set_status(format!("Playlist {name}"), false),
            Ok(()) => self.set_status(format!("Added {added} to {name}"), false),
            Err(error) => self.set_status(format!("{name} not saved: {error:#}"), true),
        }
    }

    fn open_import_prompt(&mut self) {
        let start = dirs::home_dir()
            .map(|h| format!("{}/", h.display()))
            .unwrap_or_default();
        self.name_prompt = Some(NamePrompt {
            title: "Import playlist (.m3u)",
            label: "File",
            input: start,
            paths: Vec::new(),
            kind: PromptKind::ImportM3u,
        });
    }

    fn open_export_prompt(&mut self) {
        let Some(index) = self.playlist_state.selected() else {
            return;
        };
        let Some(list) = self.playlists.get(index) else {
            return;
        };
        let safe: String = list
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = dirs::home_dir()
            .map(|h| format!("{}/", h.display()))
            .unwrap_or_default();
        self.name_prompt = Some(NamePrompt {
            title: "Export playlist (.m3u)",
            label: "File",
            input: format!("{dir}{safe}.m3u"),
            paths: Vec::new(),
            kind: PromptKind::ExportM3u(index),
        });
    }

    fn import_m3u(&mut self, input: &str) {
        let path = crate::picker::expand_home(input);
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                self.set_status(format!("Cannot read {}: {error}", path.display()), true);
                return;
            }
        };
        let base = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let tracks: Vec<PathBuf> = playlist::parse_m3u(&text, &base)
            .into_iter()
            .filter(|p| library::is_audio(p))
            .collect();
        if tracks.is_empty() {
            self.set_status("No playable files in that playlist", true);
            return;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported")
            .to_string();
        for track in &tracks {
            self.resolve_track(track);
        }
        let (index, added) = playlist::add_tracks(&mut self.playlists, &name, tracks);
        self.playlist_state.select(Some(index));
        let message = match playlist::save(&self.playlists) {
            Ok(()) => format!("Imported {added} tracks into {name}"),
            Err(error) => format!("Imported into {name} but not saved: {error:#}"),
        };
        self.set_status(message, false);
    }

    fn export_m3u(&mut self, index: usize, input: &str) {
        let Some(list) = self.playlists.get(index) else {
            return;
        };
        let name = list.name.clone();
        let paths = list.tracks.clone();
        let entries: Vec<(PathBuf, String, Option<u64>)> = paths
            .into_iter()
            .map(|path| {
                let track = self.resolve_track(&path);
                let label = if track.artist.is_empty() {
                    track.title.clone()
                } else {
                    format!("{} - {}", track.artist, track.title)
                };
                (path, label, track.duration.map(|d| d.as_secs()))
            })
            .collect();
        let target = crate::picker::expand_home(input);
        match std::fs::write(&target, playlist::to_m3u(&entries)) {
            Ok(()) => self.set_status(format!("Exported {name} to {}", target.display()), false),
            Err(error) => {
                self.set_status(format!("Cannot write {}: {error}", target.display()), true)
            }
        }
    }

    fn remove_from_playlists(&mut self) {
        match self.open_playlist {
            None => self.remove_playlist(),
            Some(list) => self.remove_playlist_track(list),
        }
    }

    fn confirm_remove(&mut self, index: usize, label: &str) -> bool {
        if self.pending_remove != Some(index) {
            self.pending_remove = Some(index);
            self.set_status(format!("Press x again to remove {label}"), false);
            return false;
        }
        self.pending_remove = None;
        true
    }

    fn remove_playlist(&mut self) {
        let Some(index) = self.playlist_state.selected() else {
            return;
        };
        let Some(name) = self.playlists.get(index).map(|p| p.name.clone()) else {
            return;
        };
        if !self.confirm_remove(index, &name) {
            return;
        }
        self.playlists.remove(index);
        self.playlist_state.select(if self.playlists.is_empty() {
            None
        } else {
            Some(index.min(self.playlists.len() - 1))
        });
        match playlist::save(&self.playlists) {
            Ok(()) => self.set_status(format!("Removed {name}"), false),
            Err(error) => self.set_status(format!("Removed but not saved: {error:#}"), true),
        }
    }

    fn remove_marked_tracks(&mut self, list: usize) {
        let count = self.marked_tracks.len();
        let label = format!("{count} tracks");
        if !self.confirm_remove(usize::MAX, &label) {
            return;
        }
        let Some(playlist) = self.playlists.get_mut(list) else {
            return;
        };
        let marked = std::mem::take(&mut self.marked_tracks);
        let mut index = 0;
        playlist.tracks.retain(|_| {
            let keep = !marked.contains(&index);
            index += 1;
            keep
        });
        let remaining = playlist.tracks.len();
        let selected = self.playlist_track_state.selected().unwrap_or(0);
        self.playlist_track_state.select(if remaining == 0 {
            None
        } else {
            Some(selected.min(remaining - 1))
        });
        match playlist::save(&self.playlists) {
            Ok(()) => self.set_status(format!("Removed {label}"), false),
            Err(error) => self.set_status(format!("Removed but not saved: {error:#}"), true),
        }
    }

    fn remove_playlist_track(&mut self, list: usize) {
        if !self.marked_tracks.is_empty() {
            self.remove_marked_tracks(list);
            return;
        }
        let Some(index) = self.playlist_track_state.selected() else {
            return;
        };
        let Some(playlist) = self.playlists.get(list) else {
            return;
        };
        let Some(track) = playlist.tracks.get(index) else {
            return;
        };
        let path = track.clone();
        let label = self.resolve_track(&path).title;
        if !self.confirm_remove(index, &label) {
            return;
        }
        let playlist = &mut self.playlists[list];
        playlist.tracks.remove(index);
        let remaining = playlist.tracks.len();
        self.playlist_track_state.select(if remaining == 0 {
            None
        } else {
            Some(index.min(remaining - 1))
        });
        match playlist::save(&self.playlists) {
            Ok(()) => self.set_status(format!("Removed {label}"), false),
            Err(error) => self.set_status(format!("Removed but not saved: {error:#}"), true),
        }
    }

    fn next_sort(&mut self) {
        let sort = self.config.library.sort.next();
        self.set_sort(sort);
    }

    fn set_sort(&mut self, sort: library::SortKey) {
        self.config.library.sort = sort;
        library::sort_tracks(&mut self.all_tracks, sort);
        self.rebuild_view();
        match config::pick_sort(sort) {
            Ok(()) => self.set_status(format!("Sort by {}", sort.name()), false),
            Err(error) => self.set_status(
                format!("Sort by {} (not saved: {error:#})", sort.name()),
                true,
            ),
        }
    }

    fn toggle_clock(&mut self) {
        let show = !self.config.clock.show;
        self.config.clock.show = show;
        if let Err(error) = config::pick_clock(show) {
            self.set_status(format!("Clock setting not saved: {error:#}"), true);
        }
    }

    fn next_visualizer(&mut self) {
        let style = self.config.visualizer.style.next();
        self.set_visualizer(style);
    }

    fn set_visualizer(&mut self, style: config::VizStyle) {
        self.config.visualizer.style = style;
        match config::pick_visualizer(style) {
            Ok(()) => self.set_status(format!("Visualizer: {}", style.name()), false),
            Err(error) => self.set_status(
                format!("Visualizer: {} (not saved: {error:#})", style.name()),
                true,
            ),
        }
    }

    fn open_finder(&mut self) {
        let mut finder = Finder::new(finder::default_country());
        finder.start();
        self.finder = Some(finder);
    }

    fn finder_key(&mut self, key: KeyEvent) {
        let Some(finder) = self.finder.as_mut() else {
            return;
        };
        match finder.on_key(key) {
            FinderOutcome::None => {}
            FinderOutcome::Close => self.finder = None,
            FinderOutcome::Play(station) => {
                self.finder = None;
                self.add_station(station);
            }
        }
    }

    fn add_station(&mut self, station: Station) {
        if let Some(index) = self.stations.iter().position(|s| s.url == station.url) {
            self.play_radio(index);
            return;
        }
        let name = station.name.clone();
        self.stations.push(station);
        let index = self.stations.len() - 1;
        match radio::save_stations(&self.stations) {
            Ok(()) => self.play_radio(index),
            Err(error) => {
                self.play_radio(index);
                self.set_status(format!("{name} not saved: {error:#}"), true);
            }
        }
    }

    fn remove_station(&mut self) {
        let Some(index) = self.radio_state.selected() else {
            return;
        };
        let Some(station) = self.stations.get(index) else {
            return;
        };
        if self.stations.len() <= 1 {
            self.set_status("Keep at least one station", true);
            return;
        }
        if self.pending_remove != Some(index) {
            let message = format!("Press x again to remove {}", station.name);
            self.pending_remove = Some(index);
            self.set_status(message, false);
            return;
        }
        self.pending_remove = None;
        let removed = self.stations.remove(index);
        match self.active {
            Active::Radio(current) if current == index => {
                self.generation += 1;
                self.engine.stop();
                self.active = Active::None;
                self.loading = false;
            }
            Active::Radio(current) if current > index => self.active = Active::Radio(current - 1),
            _ => {}
        }
        self.radio_state
            .select(Some(index.min(self.stations.len() - 1)));
        match radio::save_stations(&self.stations) {
            Ok(()) => self.set_status(format!("Removed {}", removed.name), false),
            Err(error) => self.set_status(format!("Removed but not saved: {error:#}"), true),
        }
    }

    fn seek(&mut self, forward: bool) {
        if matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        ) && !self
            .engine
            .seek_by(forward, Duration::from_secs(5), self.duration)
        {
            self.set_status("Seeking is not supported for this file", true);
        }
    }

    fn snapshot_state(&self) -> State {
        let playing = matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        );
        let (track, position_secs) = if playing {
            (self.current_path.clone(), self.position().as_secs())
        } else {
            match &self.restore {
                Some((path, pos)) => (Some(path.clone()), pos.as_secs()),
                None => (None, 0),
            }
        };
        State {
            shuffle: self.shuffle,
            repeat: self.repeat.label().to_string(),
            volume: if self.sysvol.is_none() {
                Some(self.volume)
            } else {
                None
            },
            track,
            position_secs,
            favorites: self.favorites.iter().cloned().collect(),
        }
    }

    fn autosave(&mut self) {
        if self.saved_at.elapsed() >= Duration::from_secs(20) {
            self.saved_at = Instant::now();
            state::save(&self.snapshot_state());
        }
    }

    fn media_info(&self) -> media::Info {
        match self.active {
            Active::None => media::Info {
                stopped: true,
                ..media::Info::default()
            },
            Active::Spotify => media::Info {
                title: self.sp.title.clone(),
                artist: self.sp.artist.clone(),
                album: self.sp.album.clone(),
                duration: Some(Duration::from_millis(self.sp.duration_ms as u64)),
                playing: self.sp.playing,
                position: self.position(),
                ..media::Info::default()
            },
            Active::Radio(_) => {
                let live = self.live_title();
                let (title, artist) = if live.is_empty() {
                    (self.title.clone(), "Internet radio".to_string())
                } else {
                    (live, self.title.clone())
                };
                media::Info {
                    title,
                    artist,
                    playing: self.is_playing(),
                    ..media::Info::default()
                }
            }
            Active::Local(_) | Active::Queue(_) | Active::Extra => media::Info {
                title: self.title.clone(),
                artist: self.artist.clone(),
                album: self.album.clone(),
                cover: self.cover_file.clone(),
                duration: self.duration,
                playing: self.is_playing(),
                position: self.position(),
                ..media::Info::default()
            },
        }
    }

    fn sync_media(&mut self) {
        if self.media.is_none() {
            return;
        }
        let commands = self.media.as_ref().map(Media::poll).unwrap_or_default();
        for command in commands {
            self.handle_media(command);
        }
        let info = self.media_info();
        if let Some(media) = self.media.as_mut() {
            media.update(&info);
        }
    }

    fn handle_media(&mut self, command: media::Command) {
        match command {
            media::Command::Play => {
                if !self.is_playing() {
                    self.toggle_pause();
                }
            }
            media::Command::Pause => {
                if self.is_playing() {
                    self.toggle_pause();
                }
            }
            media::Command::Toggle => self.toggle_pause(),
            media::Command::Next => self.step(true),
            media::Command::Previous => self.step(false),
            media::Command::Stop => {
                self.generation += 1;
                self.engine.stop();
                self.loading = false;
                self.pause_spotify();
                self.active = Active::None;
            }
            media::Command::SeekBy(secs) => {
                if matches!(
                    self.active,
                    Active::Local(_) | Active::Queue(_) | Active::Extra
                ) {
                    self.engine.seek_by(
                        secs >= 0,
                        Duration::from_secs(secs.unsigned_abs()),
                        self.duration,
                    );
                }
            }
            media::Command::SetPosition(position) => {
                if matches!(
                    self.active,
                    Active::Local(_) | Active::Queue(_) | Active::Extra
                ) {
                    self.engine.seek_to(position);
                }
            }
            media::Command::Quit => self.quit = true,
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(file) = self.cover_file.take() {
            let _ = std::fs::remove_file(file);
        }
        state::save(&self.snapshot_state());
        if let Some(handle) = &self.spotify {
            handle.send(SpotifyCommand::Shutdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_pick() -> Pick {
        Pick {
            track: library::to_track(PathBuf::from("/music/b.wav")),
            active: Active::Local(1),
            pop: false,
            resume: None,
            duration: None,
            epoch: 0,
        }
    }

    fn tracks(names: &[&str]) -> Vec<LocalTrack> {
        names
            .iter()
            .map(|name| library::to_track(PathBuf::from(format!("/music/{name}.wav"))))
            .collect()
    }

    #[test]
    fn preloaded_track_follows_reordered_library() {
        let pick = local_pick();
        let reordered = tracks(&["b", "c", "a"]);
        assert!(pick.resolve_active(&reordered) == Active::Local(0));
    }

    #[test]
    fn preloaded_track_follows_filtered_library() {
        let pick = local_pick();
        assert!(pick.resolve_active(&tracks(&["b"])) == Active::Local(0));
        assert!(pick.resolve_active(&tracks(&["a", "c"])) == Active::Extra);
        assert!(pick.resolve_active(&[]) == Active::Extra);
    }

    #[test]
    fn preloaded_track_follows_rescanned_library() {
        let pick = local_pick();
        assert!(pick.resolve_active(&tracks(&["new", "a", "b", "c"])) == Active::Local(2));
    }

    #[test]
    fn library_changes_preserve_playlist_and_up_next_origins() {
        let mut pick = local_pick();
        // Playlists can contain the same path more than once: keep their index.
        pick.active = Active::Queue(2);
        assert!(pick.resolve_active(&tracks(&["b"])) == Active::Queue(2));
        pick.active = Active::Extra;
        assert!(pick.resolve_active(&tracks(&["b"])) == Active::Extra);
    }
}
