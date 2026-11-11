use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::path::PathBuf;

pub enum Outcome {
    Stay,
    Cancel,
    Replace(PathBuf),
    Add(PathBuf),
    Collect(Vec<PathBuf>),
}

pub struct Picker {
    pub dir: PathBuf,
    pub entries: Vec<String>,
    pub state: ListState,
    pub show_hidden: bool,
    pub files: bool,
    pub marked: Vec<PathBuf>,
    pub input: Option<String>,
    pub error: Option<String>,
}

pub fn expand_home(text: &str) -> PathBuf {
    let trimmed = text.trim();
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = trimmed.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(trimmed)
}

impl Picker {
    pub fn new(start: PathBuf) -> Self {
        let dir = if start.is_dir() {
            start
        } else {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
        };
        let mut picker = Self {
            dir,
            entries: Vec::new(),
            state: ListState::default(),
            show_hidden: false,
            files: false,
            marked: Vec::new(),
            input: None,
            error: None,
        };
        picker.reload();
        picker.state.select(Some(0));
        picker
    }

    pub fn for_files(start: PathBuf) -> Self {
        let mut picker = Self::new(start);
        picker.files = true;
        picker.reload();
        picker
    }

    pub fn is_dir_entry(&self, name: &str) -> bool {
        name == ".." || self.dir.join(name).is_dir()
    }

    fn has_parent(&self) -> bool {
        self.dir.parent().is_some()
    }

    fn reload(&mut self) {
        self.error = None;
        let mut names: Vec<String> = Vec::new();
        match std::fs::read_dir(&self.dir) {
            Ok(read) => {
                for entry in read.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    let path = entry.path();
                    if path.is_dir() || (self.files && crate::library::is_audio(&path)) {
                        names.push(name);
                    }
                }
            }
            Err(error) => self.error = Some(format!("Cannot read folder: {error}")),
        }
        let dir = self.dir.clone();
        names.sort_by_key(|n| (!dir.join(n).is_dir(), n.to_lowercase()));
        self.entries.clear();
        if self.has_parent() {
            self.entries.push("..".to_string());
        }
        self.entries.extend(names);
        let selected = self.state.selected().unwrap_or(0);
        self.state
            .select(Some(selected.min(self.entries.len().saturating_sub(1))));
    }

    pub fn go(&mut self, path: PathBuf) -> bool {
        if !path.is_dir() {
            self.error = Some("Not a folder".to_string());
            return false;
        }
        self.dir = path.canonicalize().unwrap_or(path);
        self.state.select(Some(0));
        self.reload();
        true
    }

    fn up(&mut self) {
        let Some(parent) = self.dir.parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let child = self
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.go(parent);
        if let Some(name) = child
            && let Some(index) = self.entries.iter().position(|e| *e == name)
        {
            self.state.select(Some(index));
        }
    }

    fn enter_selected(&mut self) {
        let Some(index) = self.state.selected() else {
            return;
        };
        let Some(name) = self.entries.get(index).cloned() else {
            return;
        };
        if name == ".." {
            self.up();
        } else {
            self.go(self.dir.join(name));
        }
    }

    fn files_here(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|name| !self.is_dir_entry(name))
            .map(|name| self.dir.join(name))
            .collect()
    }

    fn toggle_mark(&mut self) {
        let target = self.highlighted();
        let on_parent = self
            .state
            .selected()
            .and_then(|i| self.entries.get(i))
            .is_none_or(|name| name == "..");
        if on_parent {
            return;
        }
        match self.marked.iter().position(|p| *p == target) {
            Some(index) => {
                self.marked.remove(index);
            }
            None => self.marked.push(target),
        }
        self.move_selection(1);
    }

    fn highlighted(&self) -> PathBuf {
        match self
            .state
            .selected()
            .and_then(|i| self.entries.get(i))
            .map(String::as_str)
        {
            Some("..") | None => self.dir.clone(),
            Some(name) => self.dir.join(name),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.entries.len() as isize - 1) as usize;
        self.state.select(Some(next));
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Esc => self.input = None,
                KeyCode::Enter => {
                    let target = expand_home(input);
                    if self.go(target) {
                        self.input = None;
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return Outcome::Stay;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Cancel,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home | KeyCode::Char('g') => self.state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self
                .state
                .select(Some(self.entries.len().saturating_sub(1))),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                let target = self.highlighted();
                if self.files && target.is_file() {
                    return Outcome::Collect(vec![target]);
                }
                self.enter_selected();
            }
            KeyCode::Char(' ') if self.files => {
                self.toggle_mark();
            }
            KeyCode::Char('a') if self.files => {
                if self.marked.is_empty() {
                    return Outcome::Collect(vec![self.highlighted()]);
                }
                return Outcome::Collect(std::mem::take(&mut self.marked));
            }
            KeyCode::Char('A') if self.files => {
                let files = self.files_here();
                if files.is_empty() {
                    self.error = Some("No audio files in this folder".to_string());
                } else {
                    return Outcome::Collect(files);
                }
            }
            KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => self.up(),
            KeyCode::Char('~') => {
                if let Some(home) = dirs::home_dir() {
                    self.go(home);
                }
            }
            KeyCode::Char('/') => self.input = Some(format!("{}/", self.dir.display())),
            KeyCode::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.reload();
            }
            KeyCode::Char(' ') => return Outcome::Replace(self.dir.clone()),
            KeyCode::Char('a') => return Outcome::Add(self.dir.clone()),
            _ => {}
        }
        Outcome::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn sandbox() -> PathBuf {
        let root = crate::testutil::temp_dir("picker");
        std::fs::create_dir_all(root.join("Beta/inner")).unwrap();
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn lists_folders() {
        let root = sandbox();
        let picker = Picker::new(root.clone());
        assert_eq!(picker.entries, vec!["..", "alpha", "Beta"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hidden_and_nav() {
        let root = sandbox();
        let mut picker = Picker::new(root.clone());
        picker.handle_key(key(KeyCode::Char('.')));
        assert!(picker.entries.contains(&".hidden".to_string()));
        picker.handle_key(key(KeyCode::Char('.')));
        picker.handle_key(key(KeyCode::Down));
        picker.handle_key(key(KeyCode::Down));
        picker.handle_key(key(KeyCode::Enter));
        assert_eq!(picker.dir, root.join("Beta"));
        assert_eq!(picker.entries, vec!["..", "inner"]);
        picker.handle_key(key(KeyCode::Backspace));
        assert_eq!(picker.dir, root);
        assert_eq!(picker.entries[picker.state.selected().unwrap()], "Beta");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replace_and_add() {
        let root = sandbox();
        let mut picker = Picker::new(root.clone());
        match picker.handle_key(key(KeyCode::Char(' '))) {
            Outcome::Replace(path) => assert_eq!(path, root),
            _ => panic!("expected replace"),
        }
        match picker.handle_key(key(KeyCode::Char('a'))) {
            Outcome::Add(path) => assert_eq!(path, root),
            _ => panic!("expected add"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn typed_path() {
        let root = sandbox();
        let mut picker = Picker::new(root.clone());
        picker.handle_key(key(KeyCode::Char('/')));
        let input = picker.input.clone().unwrap();
        for _ in 0..input.len() {
            picker.handle_key(key(KeyCode::Backspace));
        }
        for c in root.join("alpha").to_string_lossy().chars() {
            picker.handle_key(key(KeyCode::Char(c)));
        }
        picker.handle_key(key(KeyCode::Enter));
        assert_eq!(picker.dir, root.join("alpha"));
        assert!(picker.input.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_mode_collects() {
        let root = sandbox();
        crate::testutil::write_wav(&root.join("alpha/song.wav"), 1);
        crate::testutil::write_wav(&root.join("alpha/other.wav"), 1);
        std::fs::write(root.join("alpha/notes.txt"), "x").unwrap();
        let mut picker = Picker::for_files(root.join("alpha"));
        assert_eq!(picker.entries, vec!["..", "other.wav", "song.wav"]);
        picker.handle_key(key(KeyCode::Down));
        match picker.handle_key(key(KeyCode::Enter)) {
            Outcome::Collect(paths) => assert_eq!(paths, vec![root.join("alpha/other.wav")]),
            _ => panic!("expected collect"),
        }
        match picker.handle_key(key(KeyCode::Char('A'))) {
            Outcome::Collect(paths) => assert_eq!(
                paths,
                vec![root.join("alpha/other.wav"), root.join("alpha/song.wav")]
            ),
            _ => panic!("expected collect"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_mode_multi_select() {
        let root = sandbox();
        crate::testutil::write_wav(&root.join("alpha/a.wav"), 1);
        crate::testutil::write_wav(&root.join("alpha/b.wav"), 1);
        crate::testutil::write_wav(&root.join("alpha/c.wav"), 1);
        let mut picker = Picker::for_files(root.join("alpha"));
        picker.handle_key(key(KeyCode::Char(' ')));
        assert!(picker.marked.is_empty());
        picker.handle_key(key(KeyCode::Down));
        for _ in 0..3 {
            picker.handle_key(key(KeyCode::Char(' ')));
        }
        assert_eq!(picker.marked.len(), 3);
        picker.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(picker.marked.len(), 2);
        match picker.handle_key(key(KeyCode::Char('a'))) {
            Outcome::Collect(paths) => assert_eq!(
                paths,
                vec![root.join("alpha/a.wav"), root.join("alpha/b.wav")]
            ),
            _ => panic!("expected collect"),
        }
        assert!(picker.marked.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_mode_folder_listing() {
        let root = sandbox();
        let mut picker = Picker::for_files(root.clone());
        assert_eq!(picker.entries, vec!["..", "alpha", "Beta"]);
        picker.handle_key(key(KeyCode::Down));
        picker.handle_key(key(KeyCode::Char(' ')));
        match picker.handle_key(key(KeyCode::Char('a'))) {
            Outcome::Collect(paths) => assert_eq!(paths, vec![root.join("alpha")]),
            _ => panic!("expected collect"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
