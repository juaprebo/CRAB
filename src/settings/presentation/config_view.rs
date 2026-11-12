use crate::settings::infrastructure::config::Config;
use crate::settings::presentation::theme::{Theme, builtin_themes, resolve};
use crate::shell::presentation::art::Art;
use std::collections::BTreeMap;

impl Config {
    pub fn keymap(&self) -> (crate::shell::presentation::keys::Keymap, Vec<String>) {
        use crate::shell::presentation::keys::Preset;
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
        let (map, mut more) =
            crate::shell::presentation::keys::Keymap::build(preset, base, &custom);
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
        Some(self.read_art_lines().map(Art::Text).unwrap_or(Art::Crab))
    }
}
