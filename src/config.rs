use crate::art::Art;
use crate::theme::{CustomTheme, Theme, builtin_themes, resolve, toml_key};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_CONFIG: &str = r##"theme = "crab"
show_art = true
art_strength = 0.35

[visualizer]
bar_width = 2
gap = 1
peaks = true

[themes.example]
base = "synthwave"
accent = "#00e5ff"
secondary = "#7c4dff"
bars = ["#00e5ff", "#7c4dff", "#ff4081"]
"##;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct VisualizerConfig {
    pub bar_width: u16,
    pub gap: u16,
    pub peaks: bool,
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        Self {
            bar_width: 2,
            gap: 1,
            peaks: true,
        }
    }
}

impl VisualizerConfig {
    pub fn bar_width(&self) -> u16 {
        self.bar_width.clamp(1, 6)
    }

    pub fn gap(&self) -> u16 {
        self.gap.min(4)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub show_art: bool,
    pub art_file: Option<String>,
    pub art_strength: f32,
    pub visualizer: VisualizerConfig,
    pub themes: BTreeMap<String, CustomTheme>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "crab".to_string(),
            show_art: true,
            art_file: None,
            art_strength: 0.35,
            visualizer: VisualizerConfig::default(),
            themes: BTreeMap::new(),
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("crab").join("config.toml"))
}

fn is_header(line: &str) -> bool {
    line.trim_start().starts_with('[')
}

pub fn set_key(text: &str, key: &str, value: &str) -> String {
    let encoded = toml::Value::String(value.to_string()).to_string();
    let new_line = format!("{key} = {encoded}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let limit = lines
        .iter()
        .position(|l| is_header(l))
        .unwrap_or(lines.len());
    let existing = lines[..limit].iter().position(|line| {
        line.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    });
    match existing {
        Some(index) => lines[index] = new_line,
        None => lines.insert(0, new_line),
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn remove_section(text: &str, name: &str) -> String {
    let bare = format!("[themes.{}]", toml_key(name));
    let quoted = format!("[themes.\"{name}\"]");
    let mut kept: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if is_header(line) {
            let was_skipping = skipping;
            skipping = trimmed == bare || trimmed == quoted;
            if was_skipping && !skipping && kept.last().is_some_and(|l| !l.trim().is_empty()) {
                kept.push("");
            }
        }
        if !skipping {
            kept.push(line);
        }
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

pub fn put_section(text: &str, name: &str, section: &str) -> String {
    let mut out = remove_section(text, name);
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(section);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn edit_config(edit: impl FnOnce(&str) -> String) -> Result<()> {
    let path = config_path().ok_or_else(|| anyhow!("no config directory available"))?;
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = edit(&current);
    toml::from_str::<Config>(&updated).map_err(|e| anyhow!("invalid config: {}", e.message()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("cannot create config directory")?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, updated).context("cannot write config")?;
    std::fs::rename(&temp, &path).context("cannot replace config")?;
    Ok(())
}

pub fn pick_theme(name: &str) -> Result<()> {
    edit_config(|text| set_key(text, "theme", name))
}

pub fn save_theme(name: &str, section: &str) -> Result<()> {
    edit_config(|text| {
        let with_section = put_section(text, name, section);
        set_key(&with_section, "theme", name)
    })
}

pub fn delete_theme(name: &str) -> Result<()> {
    edit_config(|text| {
        let removed = remove_section(text, name);
        match toml::from_str::<Config>(&removed) {
            Ok(config) if config.theme.eq_ignore_ascii_case(name) => {
                set_key(&removed, "theme", "crab")
            }
            _ => removed,
        }
    })
}

impl Config {
    pub fn load() -> (Config, Option<String>) {
        let Some(path) = config_path() else {
            return (Config::default(), None);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => (config, None),
                Err(error) => (
                    Config::default(),
                    Some(format!("config.toml ignored: {}", error.message())),
                ),
            },
            Err(_) => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&path, DEFAULT_CONFIG);
                (Config::default(), None)
            }
        }
    }

    pub fn build_themes(&self) -> Vec<Theme> {
        let mut themes = builtin_themes();
        let builtin_count = themes.len();
        let base = themes.clone();
        for (name, custom) in &self.themes {
            let theme = resolve(name, custom, &base);
            match themes[..builtin_count]
                .iter()
                .position(|t| t.name.eq_ignore_ascii_case(name))
            {
                Some(index) => themes[index] = theme,
                None => themes.push(theme),
            }
        }
        themes
    }

    pub fn load_art(&self) -> Option<Art> {
        if !self.show_art {
            return None;
        }
        if let Some(path) = &self.art_file {
            let expanded = match path.strip_prefix("~/") {
                Some(rest) => dirs::home_dir().map(|h| h.join(rest)),
                None => Some(PathBuf::from(path)),
            };
            if let Some(text) = expanded.and_then(|p| std::fs::read_to_string(p).ok()) {
                let lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).collect();
                if !lines.is_empty() {
                    return Some(Art::Text(lines));
                }
            }
        }
        Some(Art::Crab)
    }

    pub fn art_strength(&self) -> f32 {
        self.art_strength.clamp(0.05, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(config.theme, "crab");
        assert!(config.themes.contains_key("example"));
        let themes = config.build_themes();
        assert!(themes.iter().any(|t| t.name == "example"));
        assert!(themes.iter().any(|t| t.name == "nord"));
    }

    #[test]
    fn partial_config() {
        let config: Config = toml::from_str("theme = \"nord\"").unwrap();
        assert!(config.show_art);
        assert_eq!(config.visualizer.bar_width(), 2);
    }

    #[test]
    fn set_key_cases() {
        let text = "theme = \"crab\"\nshow_art = true\n\n[themes.a]\ntheme = \"x\"\n";
        let out = set_key(text, "theme", "nord");
        assert!(out.starts_with("theme = \"nord\"\nshow_art = true"));
        assert!(out.contains("[themes.a]\ntheme = \"x\""));
        let inserted = set_key("show_art = true\n", "theme", "nord");
        assert_eq!(inserted, "theme = \"nord\"\nshow_art = true\n");
        let before_table = set_key("[visualizer]\npeaks = true\n", "theme", "nord");
        let config: Config = toml::from_str(&before_table).unwrap();
        assert_eq!(config.theme, "nord");
    }

    #[test]
    fn section_keeps_rest() {
        let text = "theme = \"crab\"\n\n[themes.keep]\naccent = \"#111111\"\n\n[themes.mine]\naccent = \"#222222\"\n\n[visualizer]\npeaks = false\n";
        let section = "[themes.mine]\naccent = \"#333333\"\n";
        let out = put_section(text, "mine", section);
        let config: Config = toml::from_str(&out).unwrap();
        assert_eq!(config.themes["mine"].accent.as_deref(), Some("#333333"));
        assert_eq!(config.themes["keep"].accent.as_deref(), Some("#111111"));
        assert!(!config.visualizer.peaks);
        assert_eq!(out.matches("[themes.mine]").count(), 1);
    }

    #[test]
    fn remove_section_leaves_the_rest() {
        let text = "theme = \"mine\"\n\n[themes.keep]\naccent = \"#111111\"\n\n[themes.mine]\naccent = \"#222222\"\n\n[visualizer]\npeaks = false\n";
        let out = remove_section(text, "mine");
        let config: Config = toml::from_str(&out).unwrap();
        assert!(!config.themes.contains_key("mine"));
        assert!(config.themes.contains_key("keep"));
        assert!(!config.visualizer.peaks);
        assert_eq!(
            remove_section("theme = \"crab\"\n", "mine"),
            "theme = \"crab\"\n"
        );
    }

    #[test]
    fn section_quoted() {
        let text = "theme = \"crab\"\n";
        let section = "[themes.\"my theme\"]\naccent = \"#333333\"\n";
        let once = put_section(text, "my theme", section);
        let twice = put_section(&once, "my theme", section);
        assert_eq!(once, twice);
        let config: Config = toml::from_str(&twice).unwrap();
        assert!(config.themes.contains_key("my theme"));
    }

    #[test]
    fn override_builtin() {
        let config: Config = toml::from_str("[themes.nord]\naccent = \"#ff0000\"\n").unwrap();
        let themes = config.build_themes();
        let nord = themes.iter().find(|t| t.name == "nord").unwrap();
        assert_eq!(nord.accent, ratatui::style::Color::Rgb(255, 0, 0));
    }
}
