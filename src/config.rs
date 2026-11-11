use crate::art::Art;
use crate::library::SortKey;
use crate::theme::{CustomTheme, Theme, builtin_themes, resolve, toml_key};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_CONFIG: &str = r##"theme = "crab"
show_art = true
art_strength = 0.35
mouse = true

[visualizer]
style = "bars"
bar_width = 2
gap = 1
peaks = true

[clock]
show = false
hour12 = false
seconds = true

[library]
sort = "path"

[audio]
replaygain = true
gapless = true
crossfade = 0.0
eq = "flat"

[scrobble]
listenbrainz_token = ""

[themes.example]
base = "synthwave"
accent = "#00e5ff"
secondary = "#7c4dff"
bars = ["#00e5ff", "#7c4dff", "#ff4081"]
"##;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VizStyle {
    #[default]
    Bars,
    Mirror,
    Line,
    Dots,
    Wave,
    Waterfall,
}

impl VizStyle {
    pub const ALL: [VizStyle; 6] = [
        VizStyle::Bars,
        VizStyle::Mirror,
        VizStyle::Line,
        VizStyle::Dots,
        VizStyle::Wave,
        VizStyle::Waterfall,
    ];

    pub fn from_name(name: &str) -> Option<VizStyle> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }

    pub fn name(self) -> &'static str {
        match self {
            VizStyle::Bars => "bars",
            VizStyle::Mirror => "mirror",
            VizStyle::Line => "line",
            VizStyle::Dots => "dots",
            VizStyle::Wave => "wave",
            VizStyle::Waterfall => "waterfall",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            VizStyle::Bars => "Spectrum",
            VizStyle::Mirror => "Spectrum - mirror",
            VizStyle::Line => "Spectrum - line",
            VizStyle::Dots => "Spectrum - dots",
            VizStyle::Wave => "Waveform",
            VizStyle::Waterfall => "Spectrogram",
        }
    }

    pub fn next(self) -> VizStyle {
        let index = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct VisualizerConfig {
    pub style: VizStyle,
    pub bar_width: u16,
    pub gap: u16,
    pub peaks: bool,
}

impl Default for VisualizerConfig {
    fn default() -> Self {
        Self {
            style: VizStyle::Bars,
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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    pub sort: SortKey,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub replaygain: bool,
    pub gapless: bool,
    pub crossfade: f32,
    pub eq: String,
    pub eq_bands: Option<Vec<f32>>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            replaygain: true,
            gapless: true,
            crossfade: 0.0,
            eq: "flat".to_string(),
            eq_bands: None,
        }
    }
}

impl AudioConfig {
    pub fn eq_gains(&self) -> [f32; crate::eq::BANDS] {
        if let Some(bands) = &self.eq_bands
            && bands.len() == crate::eq::BANDS
        {
            return std::array::from_fn(|i| bands[i]);
        }
        crate::eq::preset(&self.eq).unwrap_or([0.0; crate::eq::BANDS])
    }

    pub fn fade(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(self.crossfade.clamp(0.0, 12.0))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum KeyList {
    One(String),
    Many(Vec<String>),
}

impl KeyList {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            KeyList::One(key) => vec![key],
            KeyList::Many(keys) => keys,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct KeysConfig {
    pub preset: String,
    pub base: String,
    pub custom: BTreeMap<String, KeyList>,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            preset: "crab".to_string(),
            base: "crab".to_string(),
            custom: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ScrobbleConfig {
    pub listenbrainz_token: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub show: bool,
    pub hour12: bool,
    pub seconds: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            show: false,
            hour12: false,
            seconds: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub show_art: bool,
    pub art_file: Option<String>,
    pub art_strength: f32,
    pub mouse: bool,
    pub visualizer: VisualizerConfig,
    pub clock: ClockConfig,
    pub library: LibraryConfig,
    pub audio: AudioConfig,
    pub scrobble: ScrobbleConfig,
    pub keys: KeysConfig,
    pub themes: BTreeMap<String, CustomTheme>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "crab".to_string(),
            show_art: true,
            art_file: None,
            art_strength: 0.35,
            mouse: true,
            visualizer: VisualizerConfig::default(),
            clock: ClockConfig::default(),
            library: LibraryConfig::default(),
            audio: AudioConfig::default(),
            scrobble: ScrobbleConfig::default(),
            keys: KeysConfig::default(),
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

pub fn set_section_key(text: &str, section: &str, key: &str, value: &str) -> String {
    let encoded = toml::Value::String(value.to_string()).to_string();
    set_section_line(text, section, key, &encoded)
}

pub fn set_section_bool(text: &str, section: &str, key: &str, value: bool) -> String {
    set_section_line(text, section, key, &value.to_string())
}

fn set_section_line(text: &str, section: &str, key: &str, encoded: &str) -> String {
    let new_line = format!("{key} = {encoded}");
    let header = format!("[{section}]");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&header);
        out.push('\n');
        out.push_str(&new_line);
        out.push('\n');
        return out;
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| is_header(l))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());
    let existing = lines[start + 1..end]
        .iter()
        .position(|line| {
            line.trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .map(|offset| start + 1 + offset);
    match existing {
        Some(index) => lines[index] = new_line,
        None => lines.insert(start + 1, new_line),
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

pub fn pick_visualizer(style: VizStyle) -> Result<()> {
    edit_config(|text| set_section_key(text, "visualizer", "style", style.name()))
}

pub fn pick_sort(sort: SortKey) -> Result<()> {
    edit_config(|text| set_section_key(text, "library", "sort", sort.name()))
}

pub fn pick_custom_key(action: &str, keys: &[String], base: &str) -> Result<()> {
    let array = toml::Value::Array(
        keys.iter()
            .map(|k| toml::Value::String(k.clone()))
            .collect(),
    );
    edit_config(|text| {
        let with_key = set_section_line(text, "keys.custom", action, &array.to_string());
        let with_preset = set_section_key(&with_key, "keys", "preset", "custom");
        set_section_key(&with_preset, "keys", "base", base)
    })
}

pub fn reset_custom_key(action: &str) -> Result<()> {
    edit_config(|text| remove_section_line(text, "keys.custom", action))
}

pub fn pick_preset(name: &str) -> Result<()> {
    edit_config(|text| set_section_key(text, "keys", "preset", name))
}

fn remove_section_line(text: &str, section: &str, key: &str) -> String {
    let header = format!("[{section}]");
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        return text.to_string();
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| is_header(l))
        .map_or(lines.len(), |offset| start + 1 + offset);
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let is_target = index > start
            && index < end
            && line
                .trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='));
        if !is_target {
            out.push(*line);
        }
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

pub fn pick_eq(name: &str) -> Result<()> {
    edit_config(|text| set_section_key(text, "audio", "eq", name))
}

pub fn pick_clock(show: bool) -> Result<()> {
    edit_config(|text| set_section_bool(text, "clock", "show", show))
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

    pub fn keymap(&self) -> (crate::keys::Keymap, Vec<String>) {
        use crate::keys::Preset;
        let mut warnings = Vec::new();
        let preset = Preset::from_name(&self.keys.preset).unwrap_or_else(|| {
            warnings.push(format!("unknown key preset \"{}\"", self.keys.preset));
            Preset::Crab
        });
        let base = Preset::from_name(&self.keys.base)
            .filter(|b| *b != Preset::Custom)
            .unwrap_or(Preset::Crab);
        let custom: BTreeMap<String, Vec<String>> = self
            .keys
            .custom
            .iter()
            .map(|(name, keys)| (name.clone(), keys.clone().into_vec()))
            .collect();
        let (map, mut more) = crate::keys::Keymap::build(preset, base, &custom);
        warnings.append(&mut more);
        (map, warnings)
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
    fn style_cycle_and_names() {
        let mut style = VizStyle::Bars;
        let mut seen = Vec::new();
        for _ in 0..VizStyle::ALL.len() {
            seen.push(style.name());
            let parsed: Config =
                toml::from_str(&format!("[visualizer]\nstyle = \"{}\"\n", style.name())).unwrap();
            assert_eq!(parsed.visualizer.style, style);
            style = style.next();
        }
        assert_eq!(style, VizStyle::Bars);
        assert_eq!(
            seen,
            ["bars", "mirror", "line", "dots", "wave", "waterfall"]
        );
        assert!(toml::from_str::<Config>("[visualizer]\nstyle = \"disco\"\n").is_err());
        let default: Config = toml::from_str("").unwrap();
        assert_eq!(default.visualizer.style, VizStyle::Bars);
    }

    #[test]
    fn section_key_cases() {
        let present = "theme = \"crab\"\n\n[visualizer]\nstyle = \"bars\"\npeaks = false\n\n[themes.a]\nstyle = \"x\"\n";
        let out = set_section_key(present, "visualizer", "style", "wave");
        assert!(out.contains("[visualizer]\nstyle = \"wave\"\npeaks = false"));
        assert!(out.contains("[themes.a]\nstyle = \"x\""));
        let config: Config = toml::from_str(&out).unwrap();
        assert_eq!(config.visualizer.style, VizStyle::Wave);
        assert!(!config.visualizer.peaks);

        let missing_key = "[visualizer]\npeaks = false\n";
        let out = set_section_key(missing_key, "visualizer", "style", "dots");
        let config: Config = toml::from_str(&out).unwrap();
        assert_eq!(config.visualizer.style, VizStyle::Dots);
        assert!(!config.visualizer.peaks);

        let no_section = "theme = \"nord\"\n";
        let out = set_section_key(no_section, "visualizer", "style", "mirror");
        let config: Config = toml::from_str(&out).unwrap();
        assert_eq!(config.visualizer.style, VizStyle::Mirror);
        assert_eq!(config.theme, "nord");

        let empty = set_section_key("", "visualizer", "style", "line");
        assert_eq!(empty, "[visualizer]\nstyle = \"line\"\n");
    }

    #[test]
    fn clock_config_and_toggle() {
        let default: Config = toml::from_str("").unwrap();
        assert!(!default.clock.show && !default.clock.hour12 && default.clock.seconds);
        let parsed: Config =
            toml::from_str("[clock]\nshow = true\nhour12 = true\nseconds = false\n").unwrap();
        assert!(parsed.clock.show && parsed.clock.hour12 && !parsed.clock.seconds);

        let on = set_section_bool("theme = \"nord\"\n", "clock", "show", true);
        assert_eq!(on, "theme = \"nord\"\n\n[clock]\nshow = true\n");
        let off = set_section_bool(&on, "clock", "show", false);
        let config: Config = toml::from_str(&off).unwrap();
        assert!(!config.clock.show);
        assert_eq!(off.matches("[clock]").count(), 1);

        let both = "[visualizer]\nstyle = \"wave\"\n\n[clock]\nhour12 = true\n";
        let out = set_section_bool(both, "clock", "show", true);
        let config: Config = toml::from_str(&out).unwrap();
        assert!(config.clock.show && config.clock.hour12);
        assert_eq!(config.visualizer.style, VizStyle::Wave);
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
