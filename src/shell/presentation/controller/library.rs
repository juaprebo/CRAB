use super::*;

impl App {
    pub(super) fn start_scan(&mut self) {
        let extra: Vec<PathBuf> = self
            .playlists
            .iter()
            .flat_map(|p| p.tracks.iter().cloned())
            .collect();
        self.tracks_rx = Some(self.library.scan(
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

    pub(super) fn resolve_track(&mut self, path: &PathBuf) -> LocalTrack {
        if let Some(track) = self.meta.get(path) {
            return track.clone();
        }
        let track = self.library.read_track(path.clone());
        self.meta.insert(path.clone(), track.clone());
        track
    }

    pub fn request_autoplay(&mut self, path: PathBuf) {
        self.startup_track = Some(path);
        self.restore = None;
        self.restore_shown = true;
    }

    pub(super) fn apply_tracks(&mut self, scan: Scan) {
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
        self.library.replace_tracks(scan.tracks);
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
        if let Some(path) = self.startup_track.take() {
            if let Some(index) = self.tracks.iter().position(|track| track.path == path) {
                self.play_local(index);
            } else {
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Cannot find requested audio file: {arg0}",
                        &[("arg0", (path.display()).to_string())],
                    ),
                    true,
                );
            }
        }
    }

    pub(super) fn rebuild_view(&mut self) {
        let keep = self
            .library_state
            .selected()
            .and_then(|i| self.tracks.get(i))
            .map(|t| t.path.clone());
        let now = match self.active {
            Active::Local(i) => self.tracks.get(i).map(|t| t.path.clone()),
            _ => None,
        };
        self.tracks = self
            .library
            .visible_tracks(&self.filter, self.only_favorites);
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

    pub(super) fn refresh_browser_cover(&mut self) {
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
                let cover = library_store::read_cover(&path)
                    .as_deref()
                    .and_then(cover::decode);
                let _ = tx.send(cover);
            });
            rx
        });
    }

    pub(super) fn set_library_view(&mut self, view: View) {
        self.visual = None;
        self.marked.clear();
        self.last_click = None;
        self.browser.set_view(view, &self.tracks);
        self.refresh_browser_cover();
    }

    pub(super) fn cycle_library_view(&mut self) {
        self.visual = None;
        self.marked.clear();
        self.last_click = None;
        self.browser.cycle(&self.tracks);
        self.refresh_browser_cover();
    }

    pub(super) fn library_back(&mut self) -> bool {
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

    pub(super) fn play_browser(&mut self, open: bool) {
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
            self.browser
                .play_group(selected_song, self.library.tracks())
        };
        let Some(playback) = playback else {
            self.set_status(
                tr(
                    self.config.language,
                    "Select a track, artist or album first",
                ),
                false,
            );
            return;
        };
        let message = tr_message(
            self.config.language,
            "Playing {arg0} ({arg1} tracks)",
            &[
                ("arg0", (playback.name).to_string()),
                ("arg1", (playback.tracks.len()).to_string()),
            ],
        );
        self.queue = playback.tracks;
        self.queue_name = playback.name;
        self.queue_is_playlist = false;
        self.resume = None;
        if self.play_queue(playback.index) {
            self.set_status(message, false);
        }
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

    pub(super) fn open_picker(&mut self) {
        let start = self
            .roots
            .first()
            .cloned()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        self.picker = Some(Picker::new(start).with_language(self.config.language));
    }

    pub(super) fn picker_key(&mut self, key: KeyEvent) {
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

    pub(super) fn change_library(&mut self, roots: Vec<PathBuf>, replace: bool) {
        if replace {
            if let Active::Local(_) = self.active {
                self.engine.stop();
                self.active = Active::None;
            }
            self.tracks.clear();
            self.library.replace_tracks(Vec::new());
            self.library_state.select(None);
            self.set_library_view(self.browser.view);
        }
        self.roots = roots;
        self.watcher = library_store::watch(&self.roots);
        self.start_scan();
        self.rescan_at = None;
        self.scanning = true;
        self.tab = Tab::Library;
        library_store::save_roots(&self.roots);
        let label = self.roots_label();
        self.set_status(
            tr_message(
                self.config.language,
                "Library: {label}",
                &[("label", (label).to_string())],
            ),
            false,
        );
    }

    pub(super) fn clear_filter(&mut self) -> bool {
        if self.tab != Tab::Library || self.filter.is_empty() {
            return false;
        }
        self.filter.clear();
        self.rebuild_view();
        true
    }

    pub(super) fn filter_key(&mut self, key: KeyEvent) {
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
}
