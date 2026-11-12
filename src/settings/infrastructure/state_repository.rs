use crate::settings::domain::playback_state::State;
use std::path::PathBuf;

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

pub struct TomlStateRepository;
impl crate::settings::domain::state_repository::StateRepository for TomlStateRepository {
    fn load(&self) -> State {
        load()
    }
    fn save(&self, state: &State) {
        save(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let state = State {
            shuffle: true,
            show_stats: true,
            repeat: "all".to_string(),
            volume: Some(0.4),
            track: Some(PathBuf::from("/music/a.mp3")),
            position_secs: 93,
            favorites: vec![PathBuf::from("/music/b.mp3")],
        };
        let text = toml::to_string_pretty(&state).unwrap();
        assert_eq!(toml::from_str::<State>(&text).unwrap(), state);
        assert_eq!(toml::from_str::<State>("").unwrap(), State::default());
        let legacy = toml::from_str::<State>("shuffle = true").unwrap();
        assert!(legacy.shuffle);
        assert!(!legacy.show_stats);
    }
}
