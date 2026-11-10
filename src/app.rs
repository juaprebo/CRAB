use crate::analyzer::Spectrum;
use crate::art::Art;
use crate::config::{self, Config};
use crate::engine::{Engine, Track};
use crate::finder::{self, Finder, FinderOutcome};
use crate::library::{self, LocalTrack};
use crate::picker::{Outcome, Picker};
use crate::radio::{self, Station};
use crate::spotify::{self, SpotifyCommand, SpotifyEvent, SpotifyHandle};
use crate::sysvol::{self, Request, SystemVolume};
use crate::theme::Theme;
use crate::theme::rgb;
use crate::theme_editor::{
    Editor, EditorOutcome, Generator, GeneratorOutcome, PickerOutcome, ThemePicker,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Library,
    Radio,
    Spotify,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Library, Tab::Radio, Tab::Spotify];

    pub fn index(self) -> usize {
        match self {
            Tab::Library => 0,
            Tab::Radio => 1,
            Tab::Spotify => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Library => "Library",
            Tab::Radio => "Radio",
            Tab::Spotify => "Spotify",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Active {
    None,
    Local(usize),
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

type RadioResult = (u64, std::result::Result<Track, String>);

pub struct App {
    pub engine: Engine,
    pub spectrum: Spectrum,
    pub tracks: Vec<LocalTrack>,
    pub scanning: bool,
    pub stations: Vec<Station>,
    pub tab: Tab,
    pub library_state: ListState,
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
    pending_remove: Option<usize>,
    pub config: Config,
    pub themes: Vec<Theme>,
    pub theme_index: usize,
    pub art: Option<Art>,
    sysvol: Option<SystemVolume>,
    volume: f32,
    pub muted: bool,
    tracks_rx: Option<Receiver<Vec<LocalTrack>>>,
    roots: Vec<PathBuf>,
    watcher: Option<(notify::RecommendedWatcher, Receiver<()>)>,
    rescan_at: Option<Instant>,
    radio_tx: Sender<RadioResult>,
    radio_rx: Receiver<RadioResult>,
    generation: u64,
    radio_started: Instant,
    rng: u64,
}

impl App {
    pub fn new(mut engine: Engine, roots: Vec<PathBuf>) -> Self {
        let (config, warning) = Config::load();
        let themes = config.build_themes();
        let theme_index = themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&config.theme))
            .unwrap_or(0);
        let art = config.load_art();
        let sysvol = sysvol::start();
        let (volume, muted) = match &sysvol {
            Some(sys) => {
                engine.set_volume(1.0);
                (sys.initial.volume, sys.initial.muted)
            }
            None => (engine.volume(), false),
        };
        let (radio_tx, radio_rx) = channel();
        let stations = radio::load_stations();
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
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;
        Self {
            engine,
            spectrum: Spectrum::new(),
            tracks: Vec::new(),
            scanning: true,
            stations,
            tab: Tab::Library,
            library_state: ListState::default(),
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
            shuffle: false,
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
            pending_remove: None,
            config,
            themes,
            theme_index,
            art,
            sysvol,
            volume,
            muted,
            tracks_rx: Some(library::scan(roots.clone())),
            watcher: library::watch(&roots),
            roots,
            rescan_at: None,
            radio_tx,
            radio_rx,
            generation: 0,
            radio_started: Instant::now(),
            rng: seed,
        }
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
        match warning {
            Some(message) => self.set_status(message, true),
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
            Active::Local(_) => self.duration,
            _ => None,
        }
    }

    fn apply_tracks(&mut self, tracks: Vec<LocalTrack>) {
        let keep_path = self
            .library_state
            .selected()
            .and_then(|i| self.tracks.get(i))
            .map(|t| t.path.clone());
        let now_path = match self.active {
            Active::Local(i) => self.tracks.get(i).map(|t| t.path.clone()),
            _ => None,
        };
        self.tracks = tracks;
        let find = |path: &Option<PathBuf>, tracks: &[LocalTrack]| {
            path.as_ref()
                .and_then(|p| tracks.iter().position(|t| &t.path == p))
        };
        if now_path.is_some() {
            match find(&now_path, &self.tracks) {
                Some(index) => self.active = Active::Local(index),
                None => {
                    self.engine.stop();
                    self.active = Active::None;
                }
            }
        }
        let selection = find(&keep_path, &self.tracks).or(if self.tracks.is_empty() {
            None
        } else {
            Some(0)
        });
        self.library_state.select(selection);
    }

    pub fn live_title(&self) -> String {
        self.radio_title.lock().unwrap().clone()
    }

    pub fn tick(&mut self, bars: usize) {
        self.expire_status();
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
            self.tracks_rx = Some(library::scan(self.roots.clone()));
        }

        if let Some(rx) = &self.tracks_rx
            && let Ok(tracks) = rx.try_recv()
        {
            self.apply_tracks(tracks);
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
                Active::Local(index) if self.engine.is_finished() => self.advance_local(index),
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
            } => {
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

    pub fn play_local(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index).cloned() else {
            return;
        };
        self.pause_spotify();
        self.generation += 1;
        self.loading = false;
        match Engine::open_file(&track.path, self.engine.tap()) {
            Ok(source) => {
                self.duration = self.engine.play(source);
                self.active = Active::Local(index);
                self.title = track.title;
                self.artist = track.folder;
                self.library_state.select(Some(index));
                self.set_status("", false);
            }
            Err(error) => {
                self.engine.stop();
                self.active = Active::None;
                self.set_status(format!("{error:#}"), true);
            }
        }
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
        let tx = self.radio_tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Track> {
                let connection = radio::connect(&station.url, title)?;
                Engine::open_stream(connection.stream, &connection.mime, tap)
            })();
            let _ = tx.send((generation, result.map_err(|e| format!("{e:#}"))));
        });
    }

    fn advance_local(&mut self, current: usize) {
        if self.tracks.is_empty() {
            return;
        }
        let next = if self.shuffle {
            Some((self.next_random() % self.tracks.len() as u64) as usize)
        } else if current + 1 < self.tracks.len() {
            Some(current + 1)
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

    fn step(&mut self, forward: bool) {
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
                if let Some(index) = self.library_state.selected() {
                    self.play_local(index);
                }
            }
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
            Active::Local(_) | Active::Radio(_) => {
                if !self.loading {
                    self.engine.toggle_pause();
                }
            }
            Active::None => self.play_selected(),
        }
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Library => self.tracks.len(),
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
            Tab::Library => &mut self.library_state,
            Tab::Radio => &mut self.radio_state,
            Tab::Spotify => return,
        };
        let current = state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        state.select(Some(next));
    }

    fn select_edge(&mut self, last: bool) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let target = if last { len - 1 } else { 0 };
        match self.tab {
            Tab::Library => self.library_state.select(Some(target)),
            Tab::Radio => self.radio_state.select(Some(target)),
            Tab::Spotify => {}
        }
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
            self.library_state.select(None);
        }
        self.roots = roots;
        self.watcher = library::watch(&self.roots);
        self.tracks_rx = Some(library::scan(self.roots.clone()));
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
        if key.code != KeyCode::Char('x') {
            self.pending_remove = None;
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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('o') => self.open_picker(),
            KeyCode::Char('/') if self.tab == Tab::Radio => self.open_finder(),
            KeyCode::Char('x') if self.tab == Tab::Radio => self.remove_station(),
            KeyCode::Tab => self.switch_tab(true),
            KeyCode::BackTab => self.switch_tab(false),
            KeyCode::Char('1') => self.tab = Tab::Library,
            KeyCode::Char('2') => self.tab = Tab::Radio,
            KeyCode::Char('3') => self.tab = Tab::Spotify,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home | KeyCode::Char('g') => self.select_edge(false),
            KeyCode::End | KeyCode::Char('G') => self.select_edge(true),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('n') => self.step(true),
            KeyCode::Char('p') => self.step(false),
            KeyCode::Right | KeyCode::Char('l') => self.seek(true),
            KeyCode::Left | KeyCode::Char('h') => self.seek(false),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(0.05),
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-0.05),
            KeyCode::Char('m') => self.toggle_mute(),
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_theme_picker(),
            KeyCode::Char('r') => self.reload_config(),
            KeyCode::Char('s') => {
                self.shuffle = !self.shuffle;
                let label = if self.shuffle {
                    "Shuffle on"
                } else {
                    "Shuffle off"
                };
                self.set_status(label, false);
            }
            _ => {}
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
        if let Active::Local(_) = self.active
            && !self
                .engine
                .seek_by(forward, Duration::from_secs(5), self.duration)
        {
            self.set_status("Seeking is not supported for this file", true);
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(handle) = &self.spotify {
            handle.send(SpotifyCommand::Shutdown);
        }
    }
}
