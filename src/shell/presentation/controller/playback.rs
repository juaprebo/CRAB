use super::*;
use crate::playback::application::next_track_query::NextTrackQuery;

impl App {
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

    pub(super) fn open_track(&mut self, track: &LocalTrack, active: Active) -> bool {
        self.pause_spotify();
        self.generation += 1;
        self.loading = false;
        self.preloaded = None;
        match self
            .prepare_track
            .execute(track, self.config.audio.replaygain)
        {
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

    pub(super) fn start_scrobbler(&mut self) {
        let token = std::env::var("CRAB_LISTENBRAINZ_TOKEN")
            .unwrap_or_else(|_| self.config.scrobble.listenbrainz_token.clone());
        self.scrobbler = Scrobbler::start(&token);
    }

    pub(super) fn begin_listen(&mut self) {
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

    pub(super) fn track_listen(&mut self) {
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

    pub(super) fn apply_eq(&self) {
        self.engine.eq.set(&self.config.audio.eq_gains());
    }

    pub(super) fn cycle_eq(&mut self) {
        let name = crate::playback::infrastructure::equalizer::next_preset(&self.config.audio.eq);
        self.set_eq(name);
    }

    pub(super) fn set_eq(&mut self, name: &str) {
        self.config.audio.eq = name.to_string();
        self.config.audio.eq_bands = None;
        self.apply_eq();
        match self
            .settings_service
            .execute(PreferenceChange::Equalizer(name.into()))
        {
            Ok(()) => self.set_status(
                tr_message(
                    self.config.language,
                    "Equalizer: {name}",
                    &[("name", (name).to_string())],
                ),
                false,
            ),
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "Equalizer: {name} (not saved: {error})",
                    &[
                        ("name", (name).to_string()),
                        ("error", format!("{:#}", error)),
                    ],
                ),
                true,
            ),
        }
    }

    pub(super) fn peek_next(&mut self) -> Option<(LocalTrack, Active, bool, Option<Resume>)> {
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
        let random = if self.shuffle && self.repeat != Repeat::One {
            Some(self.next_random())
        } else {
            None
        };
        let tracks = if local { &self.tracks } else { &self.queue };
        let (index, track) = NextTrackQuery.execute(tracks, current, self.repeat, random)?;
        Some((
            track.clone(),
            if local {
                Active::Local(index)
            } else {
                Active::Queue(index)
            },
            false,
            None,
        ))
    }

    pub(super) fn commit_pick(&mut self, pick: Pick) {
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

    pub(super) fn drive_transitions(&mut self) {
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
        let source = match self
            .prepare_track
            .execute(&track, self.config.audio.replaygain)
        {
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

    pub(super) fn load_cover(&mut self, path: &std::path::Path) {
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
            let bytes = library_store::read_cover(&path);
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

    pub(super) fn open_files(&mut self, args: &[String]) {
        let paths: Vec<PathBuf> = args
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.is_file() && library_store::is_audio(path))
            .collect();
        if paths.is_empty() {
            self.set_status(tr(self.config.language, "No audio file to open"), true);
            return;
        }
        if let [path] = paths.as_slice()
            && let Some(index) = self.tracks.iter().position(|track| &track.path == path)
        {
            self.tab = Tab::Library;
            self.play_local(index);
            return;
        }
        self.queue = paths.iter().map(|path| self.resolve_track(path)).collect();
        self.queue_name = tr(self.config.language, "Opened files").to_string();
        self.queue_is_playlist = false;
        self.play_queue(0);
    }

    pub(super) fn play_queue(&mut self, index: usize) -> bool {
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

    pub(super) fn play_playlist(&mut self, list: usize, index: usize) {
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

    pub(super) fn advance_queue(&mut self, current: usize) {
        let len = self.queue.len();
        if self.repeat == Repeat::One && current < len && self.play_queue(current) {
            return;
        }
        let mut index = current;
        for _ in 0..len {
            let random = if self.shuffle {
                Some(self.next_random())
            } else {
                None
            };
            let repeat = if self.repeat == Repeat::One {
                Repeat::Off
            } else {
                self.repeat
            };
            let Some((next, _)) = NextTrackQuery.execute(&self.queue, index, repeat, random) else {
                break;
            };
            index = next;
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
        self.artist = String::from(tr(self.config.language, "Internet radio"));
        self.radio_title = Arc::new(Mutex::new(String::new()));
        self.radio_state.select(Some(index));
        self.set_status(
            tr_message(
                self.config.language,
                "Connecting to {arg0}",
                &[("arg0", (station.name).to_string())],
            ),
            false,
        );

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

    pub(super) fn advance_local(&mut self, current: usize) {
        let len = self.tracks.len();
        if len == 0 {
            return;
        }
        let random = if self.shuffle && self.repeat != Repeat::One {
            Some(self.next_random())
        } else {
            None
        };
        let next = NextTrackQuery
            .execute(&self.tracks, current, self.repeat, random)
            .map(|(index, _)| index);
        match next {
            Some(index) => self.play_local(index),
            None => {
                self.engine.stop();
                self.active = Active::None;
            }
        }
    }

    pub(super) fn start_queued(&mut self) -> bool {
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

    pub(super) fn resume_after(&mut self) {
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

    pub(super) fn step(&mut self, forward: bool) {
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

    pub(super) fn play_selected(&mut self) {
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
                            self.set_status(
                                tr(
                                    self.config.language,
                                    "This playlist is empty, press e to open it",
                                ),
                                true,
                            );
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

    pub(super) fn toggle_pause(&mut self) {
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

    pub(super) fn seek_to_ratio(&mut self, x: u16) {
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
            self.set_status(
                tr(
                    self.config.language,
                    "Seeking is not supported for this file",
                ),
                true,
            );
        }
    }

    pub(super) fn queue_add(&mut self) {
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
            tr_message(
                self.config.language,
                "Queued {count} ({arg0} up next)",
                &[
                    ("count", (count).to_string()),
                    ("arg0", (self.up_next.len()).to_string()),
                ],
            ),
            false,
        );
    }

    pub(super) fn queue_view_key(&mut self, key: KeyEvent) {
        if self.keys.action(&key) == Some(Action::QueueView) {
            self.queue_view = None;
            return;
        }
        let len = self.up_next.len();
        let Some(state) = self.queue_view.as_mut() else {
            return;
        };
        let current = state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.queue_view = None,
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

    pub(super) fn seek(&mut self, forward: bool) {
        if matches!(
            self.active,
            Active::Local(_) | Active::Queue(_) | Active::Extra
        ) && !self
            .engine
            .seek_by(forward, Duration::from_secs(5), self.duration)
        {
            self.set_status(
                tr(
                    self.config.language,
                    "Seeking is not supported for this file",
                ),
                true,
            );
        }
    }
}
