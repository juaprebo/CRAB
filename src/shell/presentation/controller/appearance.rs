use super::*;

impl App {
    pub fn theme(&self) -> &Theme {
        if let Some(editor) = &self.theme_editor {
            return &editor.draft;
        }
        if let Some(generator) = &self.theme_generator {
            return &generator.draft;
        }
        &self.themes[self.theme_index]
    }

    pub(super) fn open_theme_picker(&mut self) {
        self.theme_picker = Some(ThemePicker::new(self.theme_index));
    }

    pub(super) fn free_theme_name(&self) -> String {
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

    pub(super) fn theme_picker_key(&mut self, key: KeyEvent) {
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
                match self
                    .settings_service
                    .execute(PreferenceChange::Theme(name.clone()))
                {
                    Ok(()) => {
                        self.config.theme = name.clone();
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Theme: {name}",
                                &[("name", (name).to_string())],
                            ),
                            false,
                        );
                    }
                    Err(error) => self.set_status(
                        tr_message(
                            self.config.language,
                            "Theme {name} not saved: {error}",
                            &[
                                ("name", (name).to_string()),
                                ("error", format!("{:#}", error)),
                            ],
                        ),
                        true,
                    ),
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
                        picker.notice = Some(
                            tr(self.config.language, "Built-in themes cannot be deleted")
                                .to_string(),
                        );
                    }
                }
            }
            PickerOutcome::ConfirmDelete => {
                let name = self.themes[selected].name.clone();
                match self
                    .settings_service
                    .execute(PreferenceChange::DeleteTheme(name.clone()))
                {
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
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Deleted theme: {name}",
                                &[("name", (name).to_string())],
                            ),
                            false,
                        );
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
                self.theme_editor =
                    Some(Editor::new(&base, name, original).with_language(self.config.language));
            }
        }
    }

    pub(super) fn theme_generator_key(&mut self, key: KeyEvent) {
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
                    self.theme_editor = Some(
                        Editor::new(&generator.draft, name, generator.original)
                            .with_language(self.config.language),
                    );
                }
            }
        }
    }

    pub(super) fn theme_editor_key(&mut self, key: KeyEvent) {
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
                match self.settings_service.execute(PreferenceChange::SaveTheme {
                    name: name.clone(),
                    section: section.clone(),
                }) {
                    Ok(()) => {
                        self.theme_editor = None;
                        self.reload_config();
                        self.set_status(
                            tr_message(
                                self.config.language,
                                "Theme saved: {name}",
                                &[("name", (name).to_string())],
                            ),
                            false,
                        );
                    }
                    Err(error) => {
                        editor.naming = None;
                        editor.error = Some(format!("{error:#}"));
                    }
                }
            }
        }
    }

    pub(super) fn reload_config(&mut self) {
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
            None => self.set_status(tr(self.config.language, "Config reloaded"), false),
        }
    }

    pub(super) fn set_volume_to(&mut self, volume: f32) {
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

    pub(super) fn set_theme(&mut self, name: &str) {
        let Some(index) = self
            .themes
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        else {
            let names: Vec<&str> = self.themes.iter().map(|t| t.name.as_str()).collect();
            self.set_status(
                tr_message(
                    self.config.language,
                    "Themes: {arg0}",
                    &[("arg0", (names.join(", ")).to_string())],
                ),
                true,
            );
            return;
        };
        self.theme_index = index;
        let name = self.theme().name.clone();
        match self
            .settings_service
            .execute(PreferenceChange::Theme(name.clone()))
        {
            Ok(()) => {
                self.config.theme = name.clone();
                self.set_status(
                    tr_message(
                        self.config.language,
                        "Theme: {name}",
                        &[("name", (name).to_string())],
                    ),
                    false,
                );
            }
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "Theme {name} not saved: {error}",
                    &[
                        ("name", (name).to_string()),
                        ("error", format!("{:#}", error)),
                    ],
                ),
                true,
            ),
        }
    }

    pub(super) fn next_sort(&mut self) {
        let sort = self.config.library.sort.next();
        self.set_sort(sort);
    }

    pub(super) fn set_sort(&mut self, sort: crate::library::domain::library::SortKey) {
        self.config.library.sort = sort;
        self.library.sort(sort);
        self.rebuild_view();
        match self.settings_service.execute(PreferenceChange::Sort(sort)) {
            Ok(()) => self.set_status(
                tr_message(
                    self.config.language,
                    "Sort by {arg0}",
                    &[("arg0", tr(self.config.language, sort.name()).to_string())],
                ),
                false,
            ),
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "Sort by {arg0} (not saved: {error})",
                    &[
                        ("arg0", tr(self.config.language, sort.name()).to_string()),
                        ("error", format!("{:#}", error)),
                    ],
                ),
                true,
            ),
        }
    }

    pub(super) fn toggle_clock(&mut self) {
        let show = !self.config.clock.show;
        self.config.clock.show = show;
        if let Err(error) = self.settings_service.execute(PreferenceChange::Clock(show)) {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Clock setting not saved: {error}",
                    &[("error", format!("{:#}", error))],
                ),
                true,
            );
        }
    }

    pub(super) fn next_visualizer(&mut self) {
        let style = self.config.visualizer.style.next();
        self.set_visualizer(style);
    }

    pub(super) fn set_visualizer(&mut self, style: config::VizStyle) {
        self.config.visualizer.style = style;
        match self
            .settings_service
            .execute(PreferenceChange::Visualizer(style))
        {
            Ok(()) => self.set_status(
                tr_message(
                    self.config.language,
                    "Visualizer: {arg0}",
                    &[("arg0", (style.name()).to_string())],
                ),
                false,
            ),
            Err(error) => self.set_status(
                tr_message(
                    self.config.language,
                    "Visualizer: {arg0} (not saved: {error})",
                    &[
                        ("arg0", (style.name()).to_string()),
                        ("error", format!("{:#}", error)),
                    ],
                ),
                true,
            ),
        }
    }
}
