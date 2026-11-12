use super::visualizer::VizStyle;
use crate::library::domain::library::SortKey;
use anyhow::Result;

pub enum PreferenceChange {
    Theme(String),
    DeleteTheme(String),
    SaveTheme {
        name: String,
        section: String,
    },
    Sort(SortKey),
    Clock(bool),
    Visualizer(VizStyle),
    Equalizer(String),
    KeyPreset(String),
    CustomKey {
        action: String,
        keys: Vec<String>,
        base: String,
    },
    ResetKey(String),
}

pub trait PreferencesRepository {
    fn update(&self, change: PreferenceChange) -> Result<()>;
}
