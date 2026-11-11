use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub shuffle: bool,
    pub repeat: String,
    pub volume: Option<f32>,
    pub track: Option<PathBuf>,
    pub position_secs: u64,
}

fn state_path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::cache_dir)
        .map(|d| d.join("crab").join("state.toml"))
}

pub fn load() -> State {
    state_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str::<State>(&text).ok())
        .unwrap_or_default()
}

pub fn save(state: &State) {
    let Some(path) = state_path() else {
        return;
    };
    let Ok(text) = toml::to_string_pretty(state) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temp = path.with_extension("toml.tmp");
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let state = State {
            shuffle: true,
            repeat: "all".to_string(),
            volume: Some(0.4),
            track: Some(PathBuf::from("/music/a.mp3")),
            position_secs: 93,
        };
        let text = toml::to_string_pretty(&state).unwrap();
        assert_eq!(toml::from_str::<State>(&text).unwrap(), state);
        assert_eq!(toml::from_str::<State>("").unwrap(), State::default());
        assert!(toml::from_str::<State>("shuffle = true").unwrap().shuffle);
    }
}
