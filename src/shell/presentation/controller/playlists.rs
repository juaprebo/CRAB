use super::*;

impl App {
    fn playlist_result<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Not saved: {error}",
                        &[("error", format!("{error:#}"))],
                    ),
                    true,
                );
                None
            }
        }
    }

    pub(super) fn in_folders(&self) -> bool {
        self.tab == Tab::Library && self.browser.is_songs()
    }

    pub(super) fn in_playlists(&self) -> bool {
        self.tab == Tab::Playlists
    }

    pub(super) fn open_selected_playlist(&mut self) {
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

    pub(super) fn in_open_playlist(&self) -> bool {
        self.tab == Tab::Playlists && self.open_playlist.is_some()
    }

    pub(super) fn toggle_mark_all(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        if self.tracks.iter().all(|t| self.marked.contains(&t.path)) {
            self.marked.clear();
        } else {
            self.marked = self.tracks.iter().map(|t| t.path.clone()).collect();
        }
    }

    pub(super) fn toggle_track_mark(&mut self) {
        let Some(index) = self.playlist_track_state.selected() else {
            return;
        };
        if !self.marked_tracks.remove(&index) {
            self.marked_tracks.insert(index);
        }
        self.move_selection(1);
    }

    pub(super) fn toggle_track_mark_all(&mut self) {
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

    pub(super) fn close_playlist(&mut self) {
        if self.in_playlists() {
            self.open_playlist = None;
            self.marked_tracks.clear();
        }
    }

    pub(super) fn toggle_mark(&mut self) {
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

    pub(super) fn save_marked(&mut self) {
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

    pub(super) fn add_to_playlist(&mut self) {
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
                self.picker = Some(Picker::for_files(start).with_language(self.config.language));
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

    pub(super) fn collect_into_playlist(&mut self, paths: Vec<PathBuf>) {
        let Some(list) = self.open_playlist else {
            return;
        };
        let Some(name) = self
            .playlists
            .get(list)
            .map(|playlist| playlist.name.clone())
        else {
            return;
        };
        let found: Vec<_> = paths
            .iter()
            .flat_map(|path| library_store::collect(path))
            .collect();
        if found.is_empty() {
            if let Some(picker) = &mut self.picker {
                picker.error = Some(tr(self.config.language, "No audio files there").into());
            }
            return;
        }
        for path in &found {
            self.resolve_track(path);
        }
        let result = self
            .playlist_service
            .add_tracks(&mut self.playlists, &name, found);
        let Some((_, added)) = self.playlist_result(result) else {
            return;
        };
        if let Some(picker) = &mut self.picker {
            picker.error = None;
        }
        self.set_status(
            tr_message(
                self.config.language,
                "Added {added} to {name}",
                &[("added", added.to_string()), ("name", name)],
            ),
            false,
        );
        if self.playlist_track_state.selected().is_none() {
            self.playlist_track_state.select(Some(0));
        }
    }

    pub(super) fn name_prompt_key(&mut self, key: KeyEvent) {
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
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.input.clear()
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

    pub(super) fn rename_selected_playlist(&mut self) {
        let Some(index) = self
            .open_playlist
            .or_else(|| self.playlist_state.selected())
        else {
            return;
        };
        let Some(list) = self.playlists.get(index) else {
            return;
        };
        self.name_prompt = Some(NamePrompt {
            title: "Rename playlist",
            label: "New name",
            input: list.name.clone(),
            paths: Vec::new(),
            kind: PromptKind::Rename(index),
        });
    }

    pub(super) fn rename_playlist(&mut self, index: usize, name: &str) -> bool {
        let previous = match self
            .playlist_service
            .rename(&mut self.playlists, index, name)
        {
            Ok(previous) => previous,
            Err(error) => {
                let message = if let Some(error) =
                    error.downcast_ref::<crate::playlists::domain::playlist::PlaylistError>()
                {
                    crate::localization::presentation::messages::playlist_error(
                        self.config.language,
                        error,
                    )
                } else {
                    tr_message(
                        self.config.language,
                        "Playlist could not be renamed: {error}",
                        &[("error", format!("{error:#}"))],
                    )
                };
                self.set_status(message, true);
                return false;
            }
        };
        let name = self.playlists[index].name.clone();
        if self.queue_is_playlist && self.queue_name == previous {
            self.queue_name = name.clone();
        }
        self.set_status(
            tr_message(
                self.config.language,
                "Renamed playlist to {name}",
                &[("name", (name).to_string())],
            ),
            false,
        );
        true
    }

    pub(super) fn finish_prompt(&mut self, prompt: NamePrompt, name: String) {
        match prompt.kind {
            PromptKind::RenameFile => {
                if !self.rename_file(&prompt.paths[0], &name) {
                    self.name_prompt = Some(NamePrompt {
                        input: name,
                        ..prompt
                    });
                }
                return;
            }
            PromptKind::Rename(index) => {
                if !self.rename_playlist(index, &name) {
                    self.name_prompt = Some(NamePrompt {
                        input: name,
                        ..prompt
                    });
                }
                return;
            }
            PromptKind::ImportM3u => return self.import_m3u(&name),
            PromptKind::ExportM3u(index) => return self.export_m3u(index, &name),
            _ => {}
        }
        let new_list = prompt.kind == PromptKind::NewList;
        let result = self
            .playlist_service
            .add_tracks(&mut self.playlists, &name, prompt.paths);
        let Some((index, added)) = self.playlist_result(result) else {
            return;
        };
        let name = self.playlists[index].name.clone();
        self.playlist_state.select(Some(index));
        if new_list {
            self.tab = Tab::Playlists;
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
        }
        self.set_status(
            if new_list {
                tr_message(self.config.language, "Playlist {name}", &[("name", name)])
            } else {
                tr_message(
                    self.config.language,
                    "Added {added} to {name}",
                    &[("added", added.to_string()), ("name", name)],
                )
            },
            false,
        );
    }

    pub(super) fn open_import_prompt(&mut self) {
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

    pub(super) fn open_export_prompt(&mut self) {
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

    pub(super) fn import_m3u(&mut self, input: &str) {
        let path = crate::library::presentation::folder_picker::expand_home(input);
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Cannot read {arg0}: {error}",
                        &[
                            ("arg0", (path.display()).to_string()),
                            ("error", (error).to_string()),
                        ],
                    ),
                    true,
                );
                return;
            }
        };
        let base = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let tracks: Vec<PathBuf> = playlist_store::parse_m3u(&text, &base)
            .into_iter()
            .filter(|p| library_store::is_audio(p))
            .collect();
        if tracks.is_empty() {
            self.set_status(
                tr(self.config.language, "No playable files in that playlist"),
                true,
            );
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
        let result = self
            .playlist_service
            .add_tracks(&mut self.playlists, &name, tracks);
        let Some((index, added)) = self.playlist_result(result) else {
            return;
        };
        self.playlist_state.select(Some(index));
        self.set_status(
            tr_message(
                self.config.language,
                "Imported {added} tracks into {name}",
                &[("added", added.to_string()), ("name", name)],
            ),
            false,
        );
    }

    pub(super) fn export_m3u(&mut self, index: usize, input: &str) {
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
        let target = crate::library::presentation::folder_picker::expand_home(input);
        match std::fs::write(&target, playlist_store::to_m3u(&entries)) {
            Ok(()) => self.set_status(
                tr_message(
                    self.config.language,
                    "Exported {name} to {arg0}",
                    &[
                        ("name", (name).to_string()),
                        ("arg0", (target.display()).to_string()),
                    ],
                ),
                false,
            ),
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "Cannot write {arg0}: {error}",
                    &[
                        ("arg0", (target.display()).to_string()),
                        ("error", (error).to_string()),
                    ],
                ),
                true,
            ),
        }
    }

    pub(super) fn remove_from_playlists(&mut self) {
        match self.open_playlist {
            None => self.remove_playlist(),
            Some(list) => self.remove_playlist_track(list),
        }
    }

    pub(super) fn confirm_remove(&mut self, index: usize, label: &str) -> bool {
        if self.pending_remove != Some(index) {
            self.pending_remove = Some(index);
            self.set_status(
                tr_message(
                    self.config.language,
                    "Press x again to remove {label}",
                    &[("label", (label).to_string())],
                ),
                false,
            );
            return false;
        }
        self.pending_remove = None;
        true
    }

    pub(super) fn remove_playlist(&mut self) {
        let Some(index) = self.playlist_state.selected() else {
            return;
        };
        let Some(name) = self
            .playlists
            .get(index)
            .map(|playlist| playlist.name.clone())
        else {
            return;
        };
        if !self.confirm_remove(index, &name) {
            return;
        }
        let result = self.playlist_service.remove(&mut self.playlists, index);
        if self.playlist_result(result).is_none() {
            return;
        }
        self.playlist_state.select(if self.playlists.is_empty() {
            None
        } else {
            Some(index.min(self.playlists.len() - 1))
        });
        self.set_status(
            tr_message(self.config.language, "Removed {name}", &[("name", name)]),
            false,
        );
    }

    pub(super) fn remove_marked_tracks(&mut self, list: usize) {
        let count = self.marked_tracks.len();
        let label = tr_message(
            self.config.language,
            "{count} tracks",
            &[("count", count.to_string())],
        );
        if !self.confirm_remove(usize::MAX, &label) {
            return;
        }
        let indices: Vec<_> = self.marked_tracks.iter().copied().collect();
        let result = self
            .playlist_service
            .remove_tracks(&mut self.playlists, list, &indices);
        if self.playlist_result(result).is_none() {
            return;
        }
        self.marked_tracks.clear();
        let remaining = self.playlists[list].tracks.len();
        let selected = self.playlist_track_state.selected().unwrap_or(0);
        self.playlist_track_state.select(if remaining == 0 {
            None
        } else {
            Some(selected.min(remaining - 1))
        });
        self.set_status(
            tr_message(self.config.language, "Removed {label}", &[("label", label)]),
            false,
        );
    }

    pub(super) fn remove_playlist_track(&mut self, list: usize) {
        if !self.marked_tracks.is_empty() {
            self.remove_marked_tracks(list);
            return;
        }
        let Some(index) = self.playlist_track_state.selected() else {
            return;
        };
        let Some(path) = self
            .playlists
            .get(list)
            .and_then(|playlist| playlist.tracks.get(index))
            .cloned()
        else {
            return;
        };
        let label = self.resolve_track(&path).title;
        if !self.confirm_remove(index, &label) {
            return;
        }
        let result = self
            .playlist_service
            .remove_tracks(&mut self.playlists, list, &[index]);
        if self.playlist_result(result).is_none() {
            return;
        }
        let remaining = self.playlists[list].tracks.len();
        self.playlist_track_state.select(if remaining == 0 {
            None
        } else {
            Some(index.min(remaining - 1))
        });
        self.set_status(
            tr_message(self.config.language, "Removed {label}", &[("label", label)]),
            false,
        );
    }
}
