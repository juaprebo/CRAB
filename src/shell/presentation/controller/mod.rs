use crate::library::application::rename_file_service::RenameFileService;
use crate::radio::application::search_stations_service::SearchStationsService;
use crate::radio::application::station_service::StationService;
use crate::settings::application::playback_state_service::PlaybackStateService;
use crate::settings::{
    application::change_settings_service::ChangeSettingsService,
    domain::preferences_repository::PreferenceChange,
};
mod appearance;
mod controls;
mod library;
mod playback;
mod playlists;
use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
mod file_actions;
mod preferences;

use crate::library::application::library_service::LibraryService;
use crate::library::domain::library::LocalTrack;
use crate::library::domain::track_repository::Scan;
use crate::library::infrastructure::scanner as library_store;
use crate::library::presentation::browser::{Browser, View};
use crate::library::presentation::folder_picker::{Outcome, Picker};
use crate::localization::application::change_language_service::ChangeLanguageService;
use crate::playback::application::prepare_track_service::PrepareTrackService;
use crate::playback::domain::spectrum::Spectrum;
use crate::playback::infrastructure::artwork::{self as cover, Cover};
use crate::playback::infrastructure::engine::{Engine, Track};
use crate::playback::infrastructure::lyrics::{self, Lyrics};
use crate::playback::infrastructure::media::{self, Media};
use crate::playback::infrastructure::rodio_decoder::RodioDecoder;
use crate::playback::infrastructure::scrobble::{self as scrobble, Listen, Scrobbler};
use crate::playback::infrastructure::spotify::{self, SpotifyCommand, SpotifyEvent, SpotifyHandle};
use crate::playback::infrastructure::system_volume::{self as sysvol, Request, SystemVolume};
use crate::playlists::application::playlist_service::PlaylistService;
use crate::playlists::domain::playlist::{self as playlist, Playlist};
use crate::playlists::infrastructure::toml_repository as playlist_store;
use crate::radio::domain::station::Station;
use crate::radio::infrastructure::stream as radio;
use crate::radio::presentation::finder::{Finder, FinderOutcome};
use crate::settings::domain::playback_state::State;
use crate::settings::infrastructure::config::{self, Config};
#[cfg(test)]
use crate::settings::infrastructure::state_repository as state;
use crate::settings::presentation::theme::Theme;
use crate::settings::presentation::theme::rgb;
use crate::settings::presentation::theme_editor::{
    Editor, EditorOutcome, Generator, GeneratorOutcome, PickerOutcome, ThemePicker,
};
use crate::shell::infrastructure::process_stats::Stats;
use crate::shell::presentation::art::Art;
use crate::shell::presentation::commands::{self, SeekTo};
use crate::shell::presentation::keys::{self, Action, Key, Keymap};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet, VecDeque};
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

pub use crate::playback::domain::repeat::Repeat;

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
    Rename(usize),
    RenameFile,
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
    pub library: LibraryService,
    rename_file_service: RenameFileService,
    playlist_service: PlaylistService,
    station_service: StationService,
    search_stations: SearchStationsService,
    state_service: PlaybackStateService,
    settings_service: ChangeSettingsService,
    language_service: ChangeLanguageService,
    prepare_track: PrepareTrackService<RodioDecoder>,
    pub filter: String,
    pub filtering: bool,
    pub repeat: Repeat,
    pub up_next: VecDeque<LocalTrack>,
    resume: Option<Resume>,
    pub queue_view: Option<ListState>,
    pub help: Option<usize>,
    pub language_picker: Option<usize>,
    pub hit: Hit,
    last_click: Option<(Instant, usize)>,
    sleep_at: Option<Instant>,
    restore: Option<(PathBuf, Duration)>,
    restore_shown: bool,
    startup_track: Option<PathBuf>,
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
    pub only_favorites: bool,
    pub lyrics: Option<Lyrics>,
    pub lyrics_view: Option<usize>,
    scrobbler: Option<Scrobbler>,
    tracking: Option<Tracking>,
    preload_skip: Option<PathBuf>,
    cover_file: Option<PathBuf>,
    cover_serial: u64,
    media: Option<Media>,
    pub control: Option<crate::shell::infrastructure::control::Server>,
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
        let modules = crate::shell::infrastructure::modules::install(&engine);
        let saved = modules.saved;
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
        let stations = match modules.stations.load() {
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
        let playlists = modules.playlists.load();
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
        let tracks_rx = modules.library.scan(roots.clone(), playlists_paths, sort);
        let mut app = Self {
            engine,
            spectrum: Spectrum::new(),
            tracks: Vec::new(),
            library: modules.library,
            rename_file_service: modules.rename_file,
            playlist_service: modules.playlists,
            station_service: modules.stations,
            search_stations: modules.search_stations,
            state_service: modules.state,
            settings_service: modules.settings,
            language_service: modules.language,
            prepare_track: modules.prepare_track,
            filter: String::new(),
            filtering: false,
            repeat: Repeat::from_name(&saved.repeat),
            up_next: VecDeque::new(),
            resume: None,
            queue_view: None,
            help: None,
            language_picker: None,
            hit: Hit::default(),
            last_click: None,
            sleep_at: None,
            restore: saved
                .track
                .clone()
                .map(|p| (p, Duration::from_secs(saved.position_secs))),
            restore_shown: false,
            startup_track: None,
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
            tracks_rx: Some(tracks_rx),
            meta: HashMap::new(),
            cover: None,
            cover_tx,
            cover_rx,
            cover_for: None,
            preloaded: None,
            keys: key_map,
            stats: saved.show_stats.then(Stats::new),
            count: None,
            pending_g: false,
            visual: None,
            cmdline: None,
            cmd_cycle: None,
            cmd_history: Vec::new(),
            cmd_pos: None,
            cmd_draft: String::new(),
            only_favorites: false,
            lyrics: None,
            lyrics_view: None,
            scrobbler: None,
            tracking: None,
            preload_skip: None,
            cover_file: None,
            cover_serial: 0,
            media: Media::start(),
            control: None,
            album: String::new(),
            watcher: library_store::watch(&roots),
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
        self.serve_control();
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
                        self.set_status(
                            tr(self.config.language, "Stream interrupted, reconnecting"),
                            true,
                        );
                        self.play_radio(index);
                    } else {
                        self.active = Active::None;
                        self.set_status(tr(self.config.language, "Stream ended"), true);
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

    fn cancel_loading(&mut self) -> bool {
        if self.loading && matches!(self.active, Active::Radio(_)) {
            self.generation += 1;
            self.engine.stop();
            self.active = Active::None;
            self.loading = false;
            self.set_status(tr(self.config.language, "Connection cancelled"), false);
            return true;
        }
        false
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
            self.set_status(tr(self.config.language, "Sleep timer off"), false);
        } else {
            self.sleep_at = Some(Instant::now() + Duration::from_secs(minutes * 60));
            self.set_status(
                tr_message(
                    self.config.language,
                    "Sleep timer: {minutes} minutes",
                    &[("minutes", (minutes).to_string())],
                ),
                false,
            );
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
        self.set_status(
            tr(self.config.language, "Sleep timer: playback stopped"),
            false,
        );
    }

    fn overlay_open(&self) -> bool {
        if self.language_picker.is_some() {
            return true;
        }
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

    fn toggle_lyrics(&mut self) {
        if self.lyrics.is_some() {
            self.lyrics_view = Some(0);
        } else {
            self.set_status(tr(self.config.language, "No lyrics for this track"), false);
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

    fn open_finder(&mut self) {
        let mut finder = Finder::new(
            crate::radio::infrastructure::locale::default_country(),
            self.search_stations.clone(),
        )
        .with_language(self.config.language);
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
        let name = station.name.clone();
        match self.station_service.add(&mut self.stations, station) {
            Ok(index) => self.play_radio(index),
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "{name} not saved: {error}",
                    &[("name", name), ("error", format!("{error:#}"))],
                ),
                true,
            ),
        }
    }

    fn remove_station(&mut self) {
        let Some(index) = self.radio_state.selected() else {
            return;
        };
        let Some(station) = self.stations.get(index) else {
            return;
        };
        if self
            .station_service
            .can_remove(&self.stations, index)
            .is_err()
        {
            self.set_status(tr(self.config.language, "Keep at least one station"), true);
            return;
        }
        if self.pending_remove != Some(index) {
            let message = tr_message(
                self.config.language,
                "Press x again to remove {arg0}",
                &[("arg0", (station.name).to_string())],
            );
            self.pending_remove = Some(index);
            self.set_status(message, false);
            return;
        }
        self.pending_remove = None;
        let removed = match self.station_service.remove(&mut self.stations, index) {
            Ok(station) => station,
            Err(error) => {
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Not saved: {error}",
                        &[("error", format!("{error:#}"))],
                    ),
                    true,
                );
                return;
            }
        };
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
        self.set_status(
            tr_message(
                self.config.language,
                "Removed {arg0}",
                &[("arg0", removed.name)],
            ),
            false,
        );
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
            show_stats: self.stats.is_some(),
            repeat: self.repeat.label().to_string(),
            volume: if self.sysvol.is_none() {
                Some(self.volume)
            } else {
                None
            },
            track,
            position_secs,
            favorites: self.library.favorites().iter().cloned().collect(),
        }
    }

    fn autosave(&mut self) {
        if self.saved_at.elapsed() >= Duration::from_secs(20) {
            self.saved_at = Instant::now();
            self.state_service.save(&self.snapshot_state());
        }
    }

    #[cfg(unix)]
    fn serve_control(&mut self) {
        let mut pending = Vec::new();
        if let Some(server) = &self.control {
            while let Ok(request) = server.requests.try_recv() {
                pending.push(request);
            }
        }
        for request in pending {
            let reply = self.answer_control(&request.line);
            let _ = request.reply.send(reply);
        }
    }

    #[cfg(not(unix))]
    fn serve_control(&mut self) {}

    #[cfg(unix)]
    fn answer_control(&mut self, line: &str) -> crate::shell::infrastructure::control::Reply {
        use crate::shell::infrastructure::control::Reply;
        if line.trim() == "status" {
            return Reply {
                ok: true,
                text: self.status_json(),
            };
        }
        self.status.clear();
        self.status_is_error = false;
        self.run_command(line);
        Reply {
            ok: !self.status_is_error,
            text: self.status.clone(),
        }
    }

    #[cfg(unix)]
    fn status_json(&self) -> String {
        let info = self.media_info();
        let state = if info.stopped {
            "stopped"
        } else if info.playing {
            "playing"
        } else {
            "paused"
        };
        let seconds = |d: Duration| (d.as_secs_f64() * 10.0).round() / 10.0;
        serde_json::json!({
            "state": state,
            "title": info.title,
            "artist": info.artist,
            "album": info.album,
            "position": seconds(info.position),
            "duration": info.duration.map(seconds),
            "volume": (self.volume() * 100.0).round(),
            "muted": self.muted,
            "shuffle": self.shuffle,
            "repeat": self.repeat.label(),
        })
        .to_string()
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
                    (
                        self.title.clone(),
                        tr(self.config.language, "Internet radio").to_string(),
                    )
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
        self.state_service.save(&self.snapshot_state());
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
            track: library_store::to_track(PathBuf::from("/music/b.wav")),
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
            .map(|name| library_store::to_track(PathBuf::from(format!("/music/{name}.wav"))))
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
