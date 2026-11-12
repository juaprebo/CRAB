use super::*;
use std::path::Path;

fn replace_path(path: &mut PathBuf, source: &Path, target: &Path) {
    if path == source {
        *path = target.to_path_buf();
    }
}

fn replace_optional(path: &mut Option<PathBuf>, source: &Path, target: &Path) {
    if let Some(path) = path {
        replace_path(path, source, target);
    }
}

fn replace_track(track: &mut LocalTrack, source: &Path, renamed: &LocalTrack) {
    if track.path == source {
        *track = renamed.clone();
    }
}

impl App {
    pub(super) fn open_rename_prompt(&mut self) {
        if self.in_playlists() {
            self.rename_selected_playlist();
            return;
        }
        if self.tab != Tab::Library {
            self.set_status(
                tr(
                    self.config.language,
                    "Select a local file or playlist first",
                ),
                true,
            );
            return;
        }
        let track = if self.browser.is_songs() {
            self.library_state
                .selected()
                .and_then(|i| self.tracks.get(i))
        } else {
            self.browser
                .selected()
                .filter(|row| row.is_song())
                .and_then(|row| row.tracks.first())
        };
        let Some(track) = track else {
            self.set_status(
                tr(self.config.language, "Select an individual file to rename"),
                true,
            );
            return;
        };
        self.name_prompt = Some(NamePrompt {
            title: "Rename audio file",
            label: "New file name (keep extension)",
            input: track
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            paths: vec![track.path.clone()],
            kind: PromptKind::RenameFile,
        });
    }

    pub(super) fn rename_file(&mut self, source: &Path, name: &str) -> bool {
        let mut updated = self.playlists.clone();
        let target = match self.rename_file_service.execute(source, name, |target| {
            let mut changed = false;
            for list in &mut updated {
                for path in &mut list.tracks {
                    if path == source {
                        *path = target.to_path_buf();
                        changed = true;
                    }
                }
            }
            if changed {
                self.playlist_service.save(&updated)?;
            }
            Ok(())
        }) {
            Ok(target) => target,
            Err(error) => {
                self.set_status(
                    crate::localization::presentation::messages::file_error(
                        self.config.language,
                        &error,
                    ),
                    true,
                );
                return false;
            }
        };
        let renamed = self.library.read_track(target.clone());
        self.playlists = updated;
        self.library.rename_track(source, &renamed);
        for track in self
            .tracks
            .iter_mut()
            .chain(self.queue.iter_mut())
            .chain(self.up_next.iter_mut())
        {
            replace_track(track, source, &renamed);
        }
        self.meta.remove(source);
        self.meta.insert(target.clone(), renamed.clone());
        if self.marked.remove(source) {
            self.marked.insert(target.clone());
        }
        if let Some(visual) = &mut self.visual
            && visual.base.remove(source)
        {
            visual.base.insert(target.clone());
        }
        replace_optional(&mut self.current_path, source, &target);
        replace_optional(&mut self.startup_track, source, &target);
        replace_optional(&mut self.preload_skip, source, &target);
        if let Some((path, _)) = &mut self.restore {
            replace_path(path, source, &target);
        }
        if let Some(Resume::Local(path)) = &mut self.resume {
            replace_path(path, source, &target);
        }
        if let Some(pick) = &mut self.preloaded {
            replace_track(&mut pick.track, source, &renamed);
            if let Some(Resume::Local(path)) = &mut pick.resume {
                replace_path(path, source, &target);
            }
        }
        let mut roots_changed = false;
        for path in &mut self.roots {
            if path == source {
                *path = target.clone();
                roots_changed = true;
            }
        }
        if roots_changed {
            library_store::save_roots(&self.roots);
            self.watcher = library_store::watch(&self.roots);
        }
        if self.current_track_path() == Some(&target) {
            self.title = renamed.title.clone();
        }
        self.browser_cover_for = None;
        self.rebuild_view();
        self.state_service.save(&self.snapshot_state());
        self.start_scan();
        self.set_status(
            tr_message(
                self.config.language,
                "Renamed file to {arg0}",
                &[("arg0", (target.display()).to_string())],
            ),
            false,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires isolated XDG directories and an audio output"]
    fn rename_and_stats_survive_application_restart() {
        let sandbox =
            std::env::var("CRAB_TEST_SANDBOX").expect("isolated test environment required");
        for key in ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"] {
            assert!(Path::new(&std::env::var(key).unwrap()).starts_with(&sandbox));
        }
        let music = PathBuf::from(sandbox).join("music");
        std::fs::create_dir_all(&music).unwrap();
        let old = music.join("old.wav");
        let new = music.join("new.wav");
        crate::testutil::write_wav(&old, 3);
        let mut app = App::new(Engine::new().unwrap(), vec![music.clone()]);
        app.tracks_rx = None;
        let track = library_store::read_track(old.clone());
        app.library.replace_tracks(vec![track.clone()]);
        app.rebuild_view();
        app.playlists = vec![Playlist {
            name: "Test".into(),
            tracks: vec![old.clone(), old.clone()],
        }];
        playlist_store::save(&app.playlists).unwrap();
        app.library.set_favorite(old.clone(), true);
        app.marked.insert(old.clone());
        app.queue = vec![track.clone()];
        app.up_next.push_back(track.clone());
        app.current_path = Some(old.clone());
        app.active = Active::Local(0);
        app.restore = Some((old.clone(), Duration::from_secs(2)));
        app.resume = Some(Resume::Local(old.clone()));
        app.on_key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE));
        assert!(state::load().show_stats);
        app.on_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert!(matches!(
            app.name_prompt.as_ref().unwrap().kind,
            PromptKind::RenameFile
        ));
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for c in "new.wav".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.name_prompt.is_none(), "{}", app.status);
        assert!(new.exists() && !old.exists());
        assert_eq!(
            playlist_store::load()[0].tracks,
            vec![new.clone(), new.clone()]
        );
        assert_eq!(app.queue[0].path, new);
        assert_eq!(app.up_next[0].path, new);
        assert!(app.marked.contains(&new));
        assert_eq!(app.restore.as_ref().unwrap().0, new);
        assert!(matches!(&app.resume, Some(Resume::Local(path)) if path == &new));
        assert_eq!(state::load().favorites, vec![new.clone()]);
        assert_eq!(state::load().track, Some(new));
        app.run_command("theme nord");
        assert_eq!(Config::load().0.theme, "nord");
        app.run_command("language es");
        assert_eq!(app.config.language, Language::Es);
        assert!(app.status.starts_with("Idioma:"));
        assert_eq!(Config::load().0.language, Language::Es);
        app.run_command("language");
        assert_eq!(app.language_picker, Some(1));
        for (width, height) in [(1, 1), (20, 8), (80, 24)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::shell::presentation::terminal::draw(frame, &mut app))
                .unwrap();
            if width >= 80 {
                let buffer = terminal.backend().buffer();
                let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
                assert!(rendered.contains("Idioma"));
                assert!(rendered.contains("Español"));
            }
        }
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.config.language, Language::Es);
        app.run_command("language fr");
        assert!(app.status_is_error);
        assert_eq!(Config::load().0.language, Language::Es);
        app.shutdown();
        drop(app);
        let mut restarted = App::new(Engine::new().unwrap(), vec![music]);
        assert!(restarted.stats.is_some());
        assert_eq!(restarted.config.language, Language::Es);
        restarted.run_command("language");
        restarted.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        restarted.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(restarted.config.language, Language::En);
        assert!(restarted.language_picker.is_none());
        assert_eq!(Config::load().0.language, Language::En);
        restarted.on_key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::NONE));
        assert!(!state::load().show_stats);
        restarted.shutdown();
    }

    #[test]
    fn replaces_only_references_to_the_renamed_track() {
        let old = Path::new("/music/old.mp3");
        let target = Path::new("/music/new.mp3");
        let mut track = library_store::to_track(old.into());
        let mut other = library_store::to_track("/music/other.mp3".into());
        let renamed = library_store::to_track(target.into());
        replace_track(&mut track, old, &renamed);
        replace_track(&mut other, old, &renamed);
        assert_eq!(track.path, target);
        assert_eq!(track.title, "new");
        assert_eq!(other.path, Path::new("/music/other.mp3"));
        let mut path = Some(old.into());
        replace_optional(&mut path, old, target);
        assert_eq!(path, Some(target.into()));
    }
}
