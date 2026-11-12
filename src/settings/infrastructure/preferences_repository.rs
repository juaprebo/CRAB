use super::config;
use crate::settings::domain::preferences_repository::{PreferenceChange, PreferencesRepository};
use anyhow::Result;

pub struct TomlPreferencesRepository;
impl PreferencesRepository for TomlPreferencesRepository {
    fn update(&self, change: PreferenceChange) -> Result<()> {
        match change {
            PreferenceChange::Theme(name) => config::pick_theme(&name),
            PreferenceChange::DeleteTheme(name) => config::delete_theme(&name),
            PreferenceChange::SaveTheme { name, section } => config::save_theme(&name, &section),
            PreferenceChange::Sort(sort) => config::pick_sort(sort),
            PreferenceChange::Clock(show) => config::pick_clock(show),
            PreferenceChange::Visualizer(style) => config::pick_visualizer(style),
            PreferenceChange::Equalizer(name) => config::pick_eq(&name),
            PreferenceChange::KeyPreset(name) => config::pick_preset(&name),
            PreferenceChange::CustomKey { action, keys, base } => {
                config::pick_custom_key(&action, &keys, &base)
            }
            PreferenceChange::ResetKey(action) => config::reset_custom_key(&action),
        }
    }
}
