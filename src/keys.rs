use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    Cancel,
    Back,
    Search,
    Repeat,
    Queue,
    QueueView,
    Help,
    Mark,
    MarkAll,
    Sort,
    LibraryView,
    PlayGroup,
    TrackMark,
    TrackMarkAll,
    Save,
    Remove,
    OpenList,
    Rename,
    Import,
    Export,
    Sleep,
    Eq,
    Lyrics,
    Visualizer,
    Clock,
    Folder,
    TabNext,
    TabPrev,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Up,
    Down,
    PageUp,
    PageDown,
    HalfUp,
    HalfDown,
    Top,
    Bottom,
    Play,
    Pause,
    Next,
    Prev,
    SeekForward,
    SeekBack,
    VolumeUp,
    VolumeDown,
    Mute,
    Theme,
    Reload,
    Shuffle,
    Visual,
    Command,
    Stats,
    Favorite,
    Favorites,
}

impl Action {
    pub fn is_motion(self) -> bool {
        matches!(
            self,
            Action::Up
                | Action::Down
                | Action::PageUp
                | Action::PageDown
                | Action::HalfUp
                | Action::HalfDown
                | Action::Top
                | Action::Bottom
                | Action::Visual
                | Action::Cancel
        )
    }
}

pub const TABLE: &[(Action, &str, &[&str], &[&str])] = &[
    (Action::Quit, "quit", &["q"], &[]),
    (Action::Cancel, "cancel", &["esc"], &[]),
    (Action::Back, "back", &["backspace"], &[]),
    (Action::Search, "search", &["/"], &[]),
    (Action::Repeat, "repeat", &["R"], &[]),
    (Action::Queue, "queue", &["Q"], &[]),
    (Action::QueueView, "queue-view", &["Z"], &[]),
    (Action::Help, "help", &["?"], &[]),
    (Action::Mark, "mark", &["a"], &[]),
    (Action::MarkAll, "mark-all", &["A"], &[]),
    (Action::Sort, "sort", &["S"], &[]),
    (Action::LibraryView, "library-view", &["b"], &[]),
    (Action::PlayGroup, "play-group", &["P"], &[]),
    (Action::TrackMark, "track-mark", &["d"], &[]),
    (Action::TrackMarkAll, "track-mark-all", &["D"], &[]),
    (Action::Save, "save", &["w"], &[]),
    (Action::Remove, "remove", &["x"], &[]),
    (Action::OpenList, "open-list", &["e"], &[]),
    (Action::Rename, "rename", &["f2"], &[]),
    (Action::Import, "import", &["i"], &[]),
    (Action::Export, "export", &["E"], &[]),
    (Action::Sleep, "sleep", &["z"], &[]),
    (Action::Eq, "eq", &["y"], &[]),
    (Action::Lyrics, "lyrics", &["L"], &[]),
    (Action::Visualizer, "visualizer", &["v"], &[]),
    (Action::Clock, "clock", &["c"], &[]),
    (Action::Folder, "folder", &["o"], &[]),
    (Action::TabNext, "tab-next", &["tab"], &[]),
    (Action::TabPrev, "tab-prev", &["backtab"], &[]),
    (Action::Tab1, "tab-1", &["1"], &[]),
    (Action::Tab2, "tab-2", &["2"], &[]),
    (Action::Tab3, "tab-3", &["3"], &[]),
    (Action::Tab4, "tab-4", &["4"], &[]),
    (Action::Up, "up", &["up"], &["k"]),
    (Action::Down, "down", &["down"], &["j"]),
    (Action::PageUp, "page-up", &["pageup"], &[]),
    (Action::PageDown, "page-down", &["pagedown"], &[]),
    (Action::HalfUp, "half-up", &[], &["ctrl-u"]),
    (Action::HalfDown, "half-down", &[], &["ctrl-d"]),
    (Action::Top, "top", &["home"], &[]),
    (Action::Bottom, "bottom", &["end"], &["G"]),
    (Action::Play, "play", &["enter"], &[]),
    (Action::Pause, "pause", &["space"], &[]),
    (Action::Next, "next", &["n", ">"], &[]),
    (Action::Prev, "prev", &["p", "<"], &[]),
    (Action::SeekForward, "seek-forward", &["right"], &["l"]),
    (Action::SeekBack, "seek-back", &["left"], &["h"]),
    (Action::VolumeUp, "volume-up", &["+", "="], &[]),
    (Action::VolumeDown, "volume-down", &["-", "_"], &[]),
    (Action::Mute, "mute", &["m"], &[]),
    (Action::Theme, "theme", &["t", "T"], &[]),
    (Action::Reload, "reload", &["r"], &[]),
    (Action::Shuffle, "shuffle", &["s"], &[]),
    (Action::Visual, "visual", &[], &["V"]),
    (Action::Command, "command", &[":"], &[]),
    (Action::Stats, "stats", &["I"], &[]),
    (Action::Favorite, "favorite", &["f"], &[]),
    (Action::Favorites, "favorites", &["F"], &[]),
];

pub fn action_name(action: Action) -> &'static str {
    TABLE
        .iter()
        .find(|(a, _, _, _)| *a == action)
        .map_or("", |(_, name, _, _)| name)
}

pub fn action_by_name(name: &str) -> Option<Action> {
    let name = name.trim().to_ascii_lowercase().replace('_', "-");
    TABLE
        .iter()
        .find(|(_, n, _, _)| *n == name)
        .map(|(a, _, _, _)| *a)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    ctrl: bool,
}

impl Key {
    pub fn from_event(event: &KeyEvent) -> Option<Key> {
        if event.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let code = match event.code {
            KeyCode::Char(c) if ctrl => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        Some(Key { code, ctrl })
    }

    pub fn parse(text: &str) -> Option<Key> {
        let text = text.trim();
        let (ctrl, rest) = match text
            .strip_prefix("ctrl-")
            .or_else(|| text.strip_prefix("^"))
        {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let mut chars = rest.chars();
        let first = chars.next()?;
        if chars.next().is_none() {
            let c = if ctrl {
                first.to_ascii_lowercase()
            } else {
                first
            };
            return Some(Key {
                code: KeyCode::Char(c),
                ctrl,
            });
        }
        let code = match rest.to_ascii_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" | "shift-tab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            other => {
                let number = other.strip_prefix('f')?.parse::<u8>().ok()?;
                if !(1..=12).contains(&number) {
                    return None;
                }
                KeyCode::F(number)
            }
        };
        Some(Key { code, ctrl })
    }

    pub fn label(&self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Esc => "esc".to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::BackTab => "shift-tab".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Delete => "delete".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::PageUp => "pgup".to_string(),
            KeyCode::PageDown => "pgdn".to_string(),
            KeyCode::F(n) => format!("f{n}"),
            _ => "?".to_string(),
        };
        if self.ctrl { format!("^{base}") } else { base }
    }

    pub fn is_digit(&self) -> Option<usize> {
        match self.code {
            KeyCode::Char(c) if !self.ctrl => c.to_digit(10).map(|d| d as usize),
            _ => None,
        }
    }

    pub fn is_char(&self, c: char) -> bool {
        !self.ctrl && self.code == KeyCode::Char(c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Crab,
    Vim,
    Custom,
}

impl Preset {
    pub fn from_name(name: &str) -> Option<Preset> {
        match name.trim().to_ascii_lowercase().as_str() {
            "crab" => Some(Preset::Crab),
            "vim" => Some(Preset::Vim),
            "custom" => Some(Preset::Custom),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::Crab => "crab",
            Preset::Vim => "vim",
            Preset::Custom => "custom",
        }
    }
}

fn preset_keys(
    base: Preset,
    crab: &'static [&'static str],
    extra: &'static [&'static str],
) -> Vec<Key> {
    let mut keys: Vec<Key> = crab.iter().filter_map(|t| Key::parse(t)).collect();
    if base == Preset::Vim {
        keys.extend(extra.iter().filter_map(|t| Key::parse(t)));
    }
    keys
}

#[derive(Clone, Debug)]
pub struct Keymap {
    map: HashMap<Key, Action>,
    labels: HashMap<Action, Vec<Key>>,
    vim: bool,
}

impl Keymap {
    pub fn build(
        preset: Preset,
        base: Preset,
        custom: &BTreeMap<String, Vec<String>>,
    ) -> (Keymap, Vec<String>) {
        let base = if base == Preset::Vim {
            Preset::Vim
        } else {
            Preset::Crab
        };
        let base = if preset == Preset::Custom {
            base
        } else {
            preset
        };
        let mut warnings = Vec::new();
        let mut overrides: HashMap<Action, Vec<Key>> = HashMap::new();
        if preset == Preset::Custom {
            for (name, keys) in custom {
                let Some(action) = action_by_name(name) else {
                    warnings.push(format!("unknown action \"{name}\" in [keys.custom]"));
                    continue;
                };
                let mut parsed = Vec::new();
                for text in keys {
                    match Key::parse(text) {
                        Some(key) => parsed.push(key),
                        None => warnings.push(format!("invalid key \"{text}\" for {name}")),
                    }
                }
                if !parsed.is_empty() || keys.is_empty() {
                    overrides.insert(action, parsed);
                }
            }
        }
        let mut map = HashMap::new();
        for (action, _, crab, extra) in TABLE {
            if overrides.contains_key(action) {
                continue;
            }
            for key in preset_keys(base, crab, extra) {
                map.insert(key, *action);
            }
        }
        for (action, keys) in &overrides {
            for key in keys {
                map.insert(*key, *action);
            }
        }
        let mut labels: HashMap<Action, Vec<Key>> = HashMap::new();
        for (action, _, crab, extra) in TABLE {
            let keys = overrides
                .get(action)
                .cloned()
                .unwrap_or_else(|| preset_keys(base, crab, extra));
            let kept: Vec<Key> = keys
                .into_iter()
                .filter(|k| map.get(k) == Some(action))
                .collect();
            labels.insert(*action, kept);
        }
        (
            Keymap {
                map,
                labels,
                vim: base == Preset::Vim,
            },
            warnings,
        )
    }

    pub fn vim(&self) -> bool {
        self.vim
    }

    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        Key::from_event(event).and_then(|k| self.map.get(&k).copied())
    }

    pub fn lookup(&self, key: &Key) -> Option<Action> {
        self.map.get(key).copied()
    }

    pub fn is_bound(&self, key: &Key) -> bool {
        self.map.contains_key(key)
    }

    pub fn first(&self, action: Action) -> String {
        self.labels
            .get(&action)
            .and_then(|keys| keys.first())
            .map_or_else(|| "-".to_string(), Key::label)
    }

    pub fn all(&self, action: Action) -> String {
        let keys = self.labels.get(&action).map(Vec::as_slice).unwrap_or(&[]);
        if keys.is_empty() {
            return "-".to_string();
        }
        keys.iter().map(Key::label).collect::<Vec<_>>().join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn parses_keys() {
        assert_eq!(Key::parse("z").unwrap().label(), "z");
        assert_eq!(Key::parse("Z").unwrap().label(), "Z");
        assert_eq!(Key::parse("ctrl-D").unwrap().label(), "^d");
        assert_eq!(Key::parse("space").unwrap().label(), "space");
        assert_eq!(Key::parse("f5").unwrap().label(), "f5");
        assert!(Key::parse("f99").is_none());
        assert!(Key::parse("nonsense").is_none());
    }

    #[test]
    fn defaults_resolve() {
        let (map, warnings) = Keymap::build(Preset::Vim, Preset::Vim, &BTreeMap::new());
        assert!(warnings.is_empty());
        let press = |c| event(KeyCode::Char(c), KeyModifiers::NONE);
        assert_eq!(map.action(&press('b')), Some(Action::LibraryView));
        assert_eq!(map.action(&press('P')), Some(Action::PlayGroup));
        assert_eq!(map.action(&press('z')), Some(Action::Sleep));
        assert_eq!(map.action(&press('G')), Some(Action::Bottom));
        assert_eq!(
            map.action(&event(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
        assert_eq!(
            map.action(&event(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Action::HalfDown)
        );
        assert_eq!(map.action(&press('d')), Some(Action::TrackMark));
        assert_eq!(map.action(&press('>')), Some(Action::Next));
        assert_eq!(map.first(Action::Pause), "space");
    }

    #[test]
    fn user_overrides_replace_defaults_and_steal_keys() {
        let mut user = BTreeMap::new();
        user.insert("sleep".to_string(), vec!["Z".to_string()]);
        user.insert("bogus".to_string(), vec!["x".to_string()]);
        user.insert("eq".to_string(), vec!["??".to_string()]);
        let (map, warnings) = Keymap::build(Preset::Custom, Preset::Crab, &user);
        assert_eq!(warnings.len(), 2);
        let press = |c| event(KeyCode::Char(c), KeyModifiers::NONE);
        assert_eq!(map.action(&press('Z')), Some(Action::Sleep));
        assert_eq!(map.action(&press('z')), None);
        assert_eq!(map.first(Action::QueueView), "-");
        assert_eq!(map.first(Action::Sleep), "Z");
        assert_eq!(map.action(&press('y')), Some(Action::Eq));
    }

    #[test]
    fn crab_preset_has_no_vim_keys_and_custom_follows_its_base() {
        let press = |c| event(KeyCode::Char(c), KeyModifiers::NONE);
        let (crab, _) = Keymap::build(Preset::Crab, Preset::Vim, &BTreeMap::new());
        assert!(!crab.vim());
        assert_eq!(crab.action(&press('j')), None);
        assert_eq!(crab.action(&press('G')), None);
        assert_eq!(
            crab.action(&event(KeyCode::Down, KeyModifiers::NONE)),
            Some(Action::Down)
        );
        assert_eq!(crab.action(&press('n')), Some(Action::Next));
        let mut user = BTreeMap::new();
        user.insert("stats".to_string(), vec!["u".to_string()]);
        let (custom, warnings) = Keymap::build(Preset::Custom, Preset::Vim, &user);
        assert!(warnings.is_empty());
        assert!(custom.vim());
        assert_eq!(custom.action(&press('j')), Some(Action::Down));
        assert_eq!(custom.action(&press('u')), Some(Action::Stats));
        assert_eq!(custom.action(&press('I')), None);
        let (custom_crab, _) = Keymap::build(Preset::Custom, Preset::Crab, &user);
        assert_eq!(custom_crab.action(&press('j')), None);
        assert_eq!(Preset::from_name("VIM"), Some(Preset::Vim));
        assert_eq!(Preset::from_name("x"), None);
    }

    #[test]
    fn queue_key_stays_z_and_rename_is_available() {
        let press = |c| event(KeyCode::Char(c), KeyModifiers::NONE);
        let (crab, _) = Keymap::build(Preset::Crab, Preset::Crab, &BTreeMap::new());
        assert_eq!(crab.action(&press('V')), None);
        assert_eq!(crab.action(&press('Z')), Some(Action::QueueView));
        assert_eq!(
            crab.action(&event(KeyCode::F(2), KeyModifiers::NONE)),
            Some(Action::Rename)
        );
        let (vim, _) = Keymap::build(Preset::Vim, Preset::Vim, &BTreeMap::new());
        assert_eq!(vim.action(&press('V')), Some(Action::Visual));
        assert_eq!(vim.action(&press('Z')), Some(Action::QueueView));
    }

    #[test]
    fn names_round_trip() {
        for (action, name, _, _) in TABLE {
            assert_eq!(action_by_name(name), Some(*action));
            assert_eq!(action_name(*action), *name);
        }
        assert_eq!(action_by_name("Seek_Forward"), Some(Action::SeekForward));
    }
}
