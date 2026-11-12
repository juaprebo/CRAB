use super::*;

impl App {
    pub(super) fn library_len(&self) -> usize {
        if self.tab == Tab::Library && !self.browser.is_songs() {
            return self.browser.rows.len();
        }
        match (self.tab, self.open_playlist) {
            (Tab::Playlists, None) => self.playlists.len(),
            (Tab::Playlists, Some(list)) => self.playlists.get(list).map_or(0, |p| p.tracks.len()),
            _ => self.tracks.len(),
        }
    }

    pub(super) fn library_state_mut(&mut self) -> &mut ListState {
        if self.tab == Tab::Library && !self.browser.is_songs() {
            return &mut self.browser.state;
        }
        match (self.tab, self.open_playlist) {
            (Tab::Playlists, None) => &mut self.playlist_state,
            (Tab::Playlists, Some(_)) => &mut self.playlist_track_state,
            _ => &mut self.library_state,
        }
    }

    pub(super) fn list_len(&self) -> usize {
        match self.tab {
            Tab::Library | Tab::Playlists => self.library_len(),
            Tab::Radio => self.stations.len(),
            Tab::Spotify => 0,
        }
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
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

    pub(super) fn switch_tab(&mut self, forward: bool) {
        let index = self.tab.index();
        let count = Tab::ALL.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.tab = Tab::ALL[next];
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.language_picker.is_some() {
            self.language_picker_key(key);
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

    pub(super) fn normal_key(&mut self, key: KeyEvent) {
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

    pub(super) fn go_to(&mut self, count: Option<usize>, bottom: bool) {
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

    pub(super) fn half_page(&self) -> isize {
        ((self.hit.body.height / 2).max(1)) as isize
    }

    pub(super) fn run_action(&mut self, action: Action, count: Option<usize>) {
        let n = count.unwrap_or(1).max(1);
        match action {
            Action::Quit => self.quit = true,
            Action::Cancel => {
                if self.visual.take().is_none()
                    && !self.cancel_loading()
                    && !self.clear_filter()
                    && !self.library_back()
                {
                    self.close_playlist();
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
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Repeat {arg0}",
                        &[(
                            "arg0",
                            tr(self.config.language, self.repeat.label()).to_string(),
                        )],
                    ),
                    false,
                );
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
            Action::Rename => self.open_rename_prompt(),
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

    pub(super) fn toggle_stats(&mut self) {
        if self.stats.take().is_none() {
            let mut stats = Stats::new();
            stats.refresh();
            self.stats = Some(stats);
        }
        self.state_service.save(&self.snapshot_state());
    }

    pub fn stats_badge(&self) -> Option<String> {
        self.stats.as_ref().map(Stats::badge)
    }

    pub fn visual_active(&self) -> bool {
        self.visual.is_some()
    }

    pub(super) fn toggle_visual(&mut self) {
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
                tr(
                    self.config.language,
                    "Visual selection works in the Library and in open playlists",
                ),
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

    pub(super) fn update_visual(&mut self) {
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

    pub(super) fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
        self.set_status(
            if on {
                tr(self.config.language, "Shuffle on")
            } else {
                tr(self.config.language, "Shuffle off")
            },
            false,
        );
    }

    pub(super) fn cmdline_key(&mut self, key: KeyEvent) {
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

    pub(super) fn theme_names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }

    pub(super) fn complete_command(&mut self, input: &mut String) {
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

    pub(super) fn find_track(&self, words: &[String]) -> Option<usize> {
        let words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        self.tracks
            .iter()
            .position(|t| crate::library::domain::library::matches_filter(t, &words))
    }

    pub(super) fn set_only_favorites(&mut self, on: bool) {
        self.only_favorites = on;
        self.tab = Tab::Library;
        self.rebuild_view();
        self.set_status(
            if on {
                tr_message(
                    self.config.language,
                    "Showing {arg0} favorites",
                    &[("arg0", (self.tracks.len()).to_string())],
                )
            } else {
                tr(self.config.language, "Showing all tracks").to_string()
            },
            false,
        );
    }

    pub(super) fn toggle_favorite(&mut self) {
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
            self.set_status(
                tr(self.config.language, "Select or play a local track first"),
                true,
            );
            return;
        }
        let remove = self.library.toggle_favorites(&paths);
        let count = paths.len();
        let noun = tr(
            self.config.language,
            if count == 1 { "track" } else { "tracks" },
        );
        self.set_status(
            if remove {
                tr_message(
                    self.config.language,
                    "Removed {count} {noun} from favorites",
                    &[("count", (count).to_string()), ("noun", (noun).to_string())],
                )
            } else {
                tr_message(
                    self.config.language,
                    "Added {count} {noun} to favorites",
                    &[("count", (count).to_string()), ("noun", (noun).to_string())],
                )
            },
            false,
        );
        if remove && self.only_favorites {
            self.rebuild_view();
        }
        self.state_service.save(&self.snapshot_state());
    }

    pub(super) fn run_command(&mut self, line: &str) {
        let (name, args) = commands::split(line);
        if name.is_empty() {
            return;
        }
        let Some(command) = commands::canonical(&name) else {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Unknown command: {name}",
                    &[("name", (name).to_string())],
                ),
                true,
            );
            return;
        };
        let arg = args.first().map(String::as_str);
        match command {
            "quit" => self.quit = true,
            "help" => self.help = Some(0),
            "sort" => match arg {
                None => self.next_sort(),
                Some(text) => match crate::library::domain::library::SortKey::from_name(text) {
                    Some(sort) => self.set_sort(sort),
                    None => self.set_status(
                        tr(self.config.language, "Sort by path, title, artist or album"),
                        true,
                    ),
                },
            },
            "view" => match arg.and_then(View::from_name) {
                Some(view) => {
                    self.tab = Tab::Library;
                    self.set_library_view(view);
                }
                None => self.set_status(
                    tr(self.config.language, "Usage: view songs|artists|albums"),
                    true,
                ),
            },
            "sleep" => match arg {
                None => self.cycle_sleep(),
                Some("off") | Some("0") => self.set_sleep(0),
                Some(text) => match text.parse::<u64>() {
                    Ok(minutes) if minutes <= 600 => self.set_sleep(minutes),
                    _ => self
                        .set_status(tr(self.config.language, "Usage: sleep <minutes>|off"), true),
                },
            },
            "eq" => match arg {
                None => self.cycle_eq(),
                Some(text) => match crate::playback::infrastructure::equalizer::PRESETS
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(text))
                {
                    Some((preset, _)) => self.set_eq(preset),
                    None => {
                        let names: Vec<&str> = crate::playback::infrastructure::equalizer::PRESETS
                            .iter()
                            .map(|(n, _)| *n)
                            .collect();
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Presets: {arg0}",
                                &[("arg0", (names.join(", ")).to_string())],
                            ),
                            true,
                        );
                    }
                },
            },
            "repeat" => match arg {
                None => {
                    self.repeat = self.repeat.next();
                    self.set_status(
                        tr_message(
                            self.config.language,
                            "Repeat {arg0}",
                            &[(
                                "arg0",
                                tr(self.config.language, self.repeat.label()).to_string(),
                            )],
                        ),
                        false,
                    );
                }
                Some(text @ ("off" | "all" | "one")) => {
                    self.repeat = Repeat::from_name(text);
                    self.set_status(
                        tr_message(
                            self.config.language,
                            "Repeat {arg0}",
                            &[(
                                "arg0",
                                tr(self.config.language, self.repeat.label()).to_string(),
                            )],
                        ),
                        false,
                    );
                }
                Some(_) => {
                    self.set_status(tr(self.config.language, "Usage: repeat off|all|one"), true)
                }
            },
            "shuffle" => match arg {
                None => self.set_shuffle(!self.shuffle),
                Some("on") => self.set_shuffle(true),
                Some("off") => self.set_shuffle(false),
                Some(_) => self.set_status(tr(self.config.language, "Usage: shuffle on|off"), true),
            },
            "volume" => match arg.and_then(|a| a.trim_end_matches('%').parse::<u32>().ok()) {
                Some(percent) if percent <= 100 => self.set_volume_to(percent as f32 / 100.0),
                _ => self.set_status(tr(self.config.language, "Usage: volume 0-100"), true),
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
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Styles: {arg0}",
                                &[("arg0", (names.join(", ")).to_string())],
                            ),
                            true,
                        );
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
                    None => self.set_status(
                        tr(
                            self.config.language,
                            "Usage: tab library|playlists|radio|spotify",
                        ),
                        true,
                    ),
                }
            }
            "play" if !args.is_empty() => match self.find_track(&args) {
                Some(index) => {
                    self.tab = Tab::Library;
                    self.set_library_view(View::Songs);
                    self.library_state.select(Some(index));
                    self.play_local(index);
                }
                None => self.set_status(tr(self.config.language, "No track matches"), true),
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
                _ => self.set_status(
                    tr(
                        self.config.language,
                        "Usage: goto <text> (no track matches)",
                    ),
                    true,
                ),
            },
            "find" => {
                self.tab = Tab::Library;
                self.filter = args.join(" ");
                self.rebuild_view();
                let text = if self.filter.is_empty() {
                    tr(self.config.language, "Filter cleared").to_string()
                } else {
                    tr_message(
                        self.config.language,
                        "{arg0} matches",
                        &[("arg0", (self.tracks.len()).to_string())],
                    )
                };
                self.set_status(text, false);
            }
            "add" => {
                let words: Vec<String> = args.iter().map(|w| w.to_lowercase()).collect();
                if words.is_empty() {
                    self.set_status(tr(self.config.language, "Usage: add <text>"), true);
                } else {
                    let found: Vec<LocalTrack> = self
                        .library
                        .tracks()
                        .iter()
                        .filter(|t| crate::library::domain::library::matches_filter(t, &words))
                        .cloned()
                        .collect();
                    if found.is_empty() {
                        self.set_status(tr(self.config.language, "No track matches"), true);
                    } else {
                        let count = found.len();
                        self.up_next.extend(found);
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
                }
            }
            "mute" => self.toggle_mute(),
            "fade" => match arg {
                None => {
                    let secs = self.config.audio.crossfade;
                    self.set_status(
                        tr_message(
                            self.config.language,
                            "Crossfade {secs} s",
                            &[("secs", (secs).to_string())],
                        ),
                        false,
                    );
                }
                Some("off") => {
                    self.config.audio.crossfade = 0.0;
                    self.set_status(tr(self.config.language, "Crossfade off"), false);
                }
                Some(text) => match text.trim_end_matches('s').parse::<f32>() {
                    Ok(secs) if (0.0..=12.0).contains(&secs) => {
                        self.config.audio.crossfade = secs;
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Crossfade {secs} s",
                                &[("secs", (secs).to_string())],
                            ),
                            false,
                        );
                    }
                    _ => self.set_status(tr(self.config.language, "Usage: fade 0-12|off"), true),
                },
            },
            "fav" => self.toggle_favorite(),
            "favs" => match arg {
                Some("on") => self.set_only_favorites(true),
                Some("off") => self.set_only_favorites(false),
                None => self.set_only_favorites(!self.only_favorites),
                Some(_) => self.set_status(tr(self.config.language, "Usage: favs on|off"), true),
            },
            "pause" => {
                if self.is_playing() {
                    self.toggle_pause();
                }
            }
            "next" => self.step(true),
            "prev" => self.step(false),
            "stop" => self.handle_media(crate::playback::infrastructure::media::Command::Stop),
            "seek" => match arg.and_then(commands::parse_seek) {
                Some(SeekTo::Absolute(position)) => self.handle_media(
                    crate::playback::infrastructure::media::Command::SetPosition(position),
                ),
                Some(SeekTo::Relative(secs)) => self.handle_media(
                    crate::playback::infrastructure::media::Command::SeekBy(secs),
                ),
                None => self.set_status(
                    tr(self.config.language, "Usage: seek 1:30 | +10 | -10"),
                    true,
                ),
            },
            "queue" => self.queue_add(),
            "rename" if self.tab == Tab::Library => {
                self.open_rename_prompt();
                if !args.is_empty()
                    && let Some(prompt) = self.name_prompt.take()
                {
                    self.finish_prompt(prompt, args.join(" "));
                }
            }
            "rename" if self.in_playlists() => {
                if args.is_empty() {
                    self.rename_selected_playlist();
                } else if let Some(index) = self
                    .open_playlist
                    .or_else(|| self.playlist_state.selected())
                {
                    self.rename_playlist(index, &args.join(" "));
                }
            }
            "rename" => self.set_status(
                tr(
                    self.config.language,
                    "Select a local file or playlist first",
                ),
                true,
            ),
            "language" => match arg {
                None => {
                    self.language_picker = Some(
                        Language::ALL
                            .iter()
                            .position(|language| *language == self.config.language)
                            .unwrap_or(0),
                    )
                }
                Some(value) => match Language::parse(value) {
                    Some(language) => self.set_language(language),
                    None => {
                        self.set_status(tr(self.config.language, "Usage: language en|es"), true)
                    }
                },
            },
            "stats" => {
                if self.stats.is_none() {
                    self.toggle_stats();
                }
                let detail = self
                    .stats
                    .as_ref()
                    .map(|stats| stats.detail(self.config.language))
                    .unwrap_or_default();
                self.set_status(detail, false);
            }
            "keys" => self.keys_command(arg),
            "reset" => match arg {
                Some(action) => self.reset_key(action),
                None => self.set_status(tr(self.config.language, "Usage: reset <action>"), true),
            },
            "map" => self.map_command(&args),
            "unmap" => match args.first() {
                Some(action) => self.bind_keys(action, Vec::new()),
                None => self.set_status(tr(self.config.language, "Usage: unmap <action>"), true),
            },
            _ => {}
        }
    }

    pub(super) fn map_command(&mut self, args: &[String]) {
        match args {
            [] => self.set_status(
                tr(self.config.language, "Usage: map <action> <key>..."),
                true,
            ),
            [name] => match keys::action_by_name(name) {
                Some(action) => {
                    let text = format!("{}: {}", keys::action_name(action), self.keys.all(action));
                    self.set_status(text, false);
                }
                None => self.set_status(
                    tr_message(
                        self.config.language,
                        "Unknown action: {name}",
                        &[("name", (name).to_string())],
                    ),
                    true,
                ),
            },
            [name, rest @ ..] => self.bind_keys(name, rest.to_vec()),
        }
    }

    pub(super) fn bind_keys(&mut self, name: &str, keys_list: Vec<String>) {
        let Some(action) = keys::action_by_name(name) else {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Unknown action: {name}",
                    &[("name", (name).to_string())],
                ),
                true,
            );
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
                    self.set_status(
                        tr_message(
                            self.config.language,
                            "Invalid key: {text}",
                            &[("text", (text).to_string())],
                        ),
                        true,
                    );
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
        if let Err(error) = self.settings_service.execute(PreferenceChange::CustomKey {
            action: action_name.into(),
            keys: keys_list.clone(),
            base: base.clone(),
        }) {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Not saved: {error}",
                    &[("error", format!("{:#}", error))],
                ),
                true,
            );
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
            tr_message(
                self.config.language,
                "{action_name} unbound (custom keys, base {base})",
                &[
                    ("action_name", (action_name).to_string()),
                    ("base", (base).to_string()),
                ],
            )
        } else {
            tr_message(
                self.config.language,
                "{action_name}: {arg0} (custom keys, base {base})",
                &[
                    ("action_name", (action_name).to_string()),
                    ("arg0", (keys_list.join(" ")).to_string()),
                    ("base", (base).to_string()),
                ],
            )
        };
        stolen.dedup();
        if !stolen.is_empty() {
            text.push_str(&tr_message(
                self.config.language,
                ", taken from {arg0}",
                &[("arg0", (stolen.join(", ")).to_string())],
            ));
        }
        self.set_status(text, false);
    }

    pub(super) fn keys_command(&mut self, arg: Option<&str>) {
        let Some(name) = arg else {
            let text = tr_message(
                self.config.language,
                "Key preset: {arg0}",
                &[("arg0", (self.config.keys.preset).to_string())],
            );
            self.set_status(text, false);
            return;
        };
        let Some(preset) = keys::Preset::from_name(name) else {
            self.set_status(
                tr(self.config.language, "Usage: keys crab|vim|custom"),
                true,
            );
            return;
        };
        if let Err(error) = self
            .settings_service
            .execute(PreferenceChange::KeyPreset(preset.name().into()))
        {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Not saved: {error}",
                    &[("error", format!("{:#}", error))],
                ),
                true,
            );
            return;
        }
        self.config.keys.preset = preset.name().to_string();
        let (map, warnings) = self.config.keymap();
        self.keys = map;
        match warnings.first() {
            Some(warning) => self.set_status(warning.clone(), true),
            None => self.set_status(
                tr_message(
                    self.config.language,
                    "Key preset: {arg0}",
                    &[("arg0", (preset.name()).to_string())],
                ),
                false,
            ),
        }
    }

    pub(super) fn reset_key(&mut self, name: &str) {
        let Some(action) = keys::action_by_name(name) else {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Unknown action: {name}",
                    &[("name", (name).to_string())],
                ),
                true,
            );
            return;
        };
        let action_name = keys::action_name(action);
        if let Err(error) = self
            .settings_service
            .execute(PreferenceChange::ResetKey(action_name.into()))
        {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Not saved: {error}",
                    &[("error", format!("{:#}", error))],
                ),
                true,
            );
            return;
        }
        self.config.keys.custom.remove(action_name);
        let (map, _) = self.config.keymap();
        self.keys = map;
        self.set_status(
            tr_message(
                self.config.language,
                "{action_name} back to the base keys",
                &[("action_name", (action_name).to_string())],
            ),
            false,
        );
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

    pub(super) fn click_row(&mut self, y: u16) {
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
}
