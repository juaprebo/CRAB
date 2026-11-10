use crate::theme::{Theme, from_hsl, generate, parse_hex, rgb, to_color, to_hex, to_hsl};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;

pub const FIXED_ROWS: usize = 10;
pub const MIN_BARS: usize = 2;
pub const MAX_BARS: usize = 6;

const LABELS: [&str; FIXED_ROWS] = [
    "background",
    "text",
    "dim",
    "border",
    "accent",
    "secondary",
    "on accent",
    "error",
    "ok",
    "art",
];

pub enum PickerOutcome {
    Stay,
    Cancel,
    Apply,
    Edit,
    New,
    Delete,
    ConfirmDelete,
}

pub struct ThemePicker {
    pub state: ListState,
    pub original: usize,
    pub confirm: bool,
    pub notice: Option<String>,
}

impl ThemePicker {
    pub fn new(current: usize) -> Self {
        let mut state = ListState::default();
        state.select(Some(current));
        Self {
            state,
            original: current,
            confirm: false,
            notice: None,
        }
    }

    pub fn selected(&self) -> usize {
        self.state.selected().unwrap_or(0)
    }

    pub fn handle_key(&mut self, key: KeyEvent, count: usize) -> PickerOutcome {
        self.notice = None;
        if self.confirm {
            self.confirm = false;
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => PickerOutcome::ConfirmDelete,
                _ => PickerOutcome::Stay,
            };
        }
        let last = count.saturating_sub(1);
        let current = self.selected();
        let target = match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('t') => {
                return PickerOutcome::Cancel;
            }
            KeyCode::Enter => return PickerOutcome::Apply,
            KeyCode::Char('e') => return PickerOutcome::Edit,
            KeyCode::Char('n') => return PickerOutcome::New,
            KeyCode::Char('x') | KeyCode::Delete => return PickerOutcome::Delete,
            KeyCode::Up | KeyCode::Char('k') => current.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (current + 1).min(last),
            KeyCode::PageUp => current.saturating_sub(5),
            KeyCode::PageDown => (current + 5).min(last),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => last,
            _ => return PickerOutcome::Stay,
        };
        self.state.select(Some(target));
        PickerOutcome::Stay
    }
}

#[derive(Clone, Copy)]
pub struct ColorPicker {
    pub h: f32,
    pub s: f32,
    pub l: f32,
}

pub enum PickKey {
    Moved,
    Done,
    Cancel,
    Hex,
}

impl ColorPicker {
    pub fn from_rgb(color: (u8, u8, u8)) -> Self {
        let (h, s, l) = to_hsl(color);
        Self { h, s, l }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        from_hsl(self.h, self.s, self.l)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PickKey {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.h = (self.h - 4.0).rem_euclid(360.0),
            KeyCode::Right | KeyCode::Char('l') => self.h = (self.h + 4.0).rem_euclid(360.0),
            KeyCode::Up | KeyCode::Char('k') => self.l = (self.l + 0.02).min(1.0),
            KeyCode::Down | KeyCode::Char('j') => self.l = (self.l - 0.02).max(0.0),
            KeyCode::PageUp => self.s = (self.s + 0.05).min(1.0),
            KeyCode::PageDown => self.s = (self.s - 0.05).max(0.0),
            KeyCode::Enter => return PickKey::Done,
            KeyCode::Esc | KeyCode::Char('q') => return PickKey::Cancel,
            KeyCode::Char('#') => return PickKey::Hex,
            _ => {}
        }
        PickKey::Moved
    }
}

pub enum GeneratorOutcome {
    Stay,
    Cancel,
    Accept,
}

pub struct Generator {
    pub picker: ColorPicker,
    pub dark: bool,
    pub draft: Theme,
    pub original: usize,
}

impl Generator {
    pub fn new(accent: (u8, u8, u8), dark: bool, original: usize) -> Self {
        Self {
            picker: ColorPicker::from_rgb(accent),
            dark,
            draft: generate(accent, dark),
            original,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> GeneratorOutcome {
        if matches!(key.code, KeyCode::Char('m') | KeyCode::Tab) {
            self.dark = !self.dark;
        } else {
            match self.picker.handle_key(key) {
                PickKey::Done => return GeneratorOutcome::Accept,
                PickKey::Cancel => return GeneratorOutcome::Cancel,
                PickKey::Moved | PickKey::Hex => {}
            }
        }
        self.draft = generate(self.picker.rgb(), self.dark);
        GeneratorOutcome::Stay
    }
}

pub struct EditingColor {
    pub picker: ColorPicker,
    pub before: Option<(u8, u8, u8)>,
}

pub enum EditorOutcome {
    Stay,
    Cancel,
    Save(String),
}

pub struct Editor {
    pub draft: Theme,
    pub name: String,
    pub selected: usize,
    pub input: Option<String>,
    pub naming: Option<String>,
    pub color: Option<EditingColor>,
    pub error: Option<String>,
    pub original: usize,
}

pub fn valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= 32
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
}

impl Editor {
    pub fn new(base: &Theme, name: String, original: usize) -> Self {
        let mut draft = base.clone();
        draft.name = name.clone();
        Self {
            draft,
            name,
            selected: 4,
            input: None,
            naming: None,
            color: None,
            error: None,
            original,
        }
    }

    pub fn rows(&self) -> usize {
        FIXED_ROWS + self.draft.bars.len()
    }

    pub fn label(&self, row: usize) -> String {
        if row < FIXED_ROWS {
            LABELS[row].to_string()
        } else {
            format!("bar {}", row - FIXED_ROWS + 1)
        }
    }

    pub fn optional(row: usize) -> bool {
        row == 0 || row == 9
    }

    pub fn get(&self, row: usize) -> Option<(u8, u8, u8)> {
        let theme = &self.draft;
        match row {
            0 => theme.background.map(rgb),
            1 => Some(rgb(theme.text)),
            2 => Some(rgb(theme.dim)),
            3 => Some(rgb(theme.border)),
            4 => Some(rgb(theme.accent)),
            5 => Some(rgb(theme.secondary)),
            6 => Some(rgb(theme.on_accent)),
            7 => Some(rgb(theme.error)),
            8 => Some(rgb(theme.ok)),
            9 => theme.art.map(rgb),
            n => theme.bars.get(n - FIXED_ROWS).copied(),
        }
    }

    pub fn set(&mut self, row: usize, value: Option<(u8, u8, u8)>) {
        let theme = &mut self.draft;
        let color = value.map(to_color);
        match (row, color, value) {
            (0, color, _) => theme.background = color,
            (9, color, _) => theme.art = color,
            (1, Some(c), _) => theme.text = c,
            (2, Some(c), _) => theme.dim = c,
            (3, Some(c), _) => theme.border = c,
            (4, Some(c), _) => theme.accent = c,
            (5, Some(c), _) => theme.secondary = c,
            (6, Some(c), _) => theme.on_accent = c,
            (7, Some(c), _) => theme.error = c,
            (8, Some(c), _) => theme.ok = c,
            (n, _, Some(rgb)) if n >= FIXED_ROWS => {
                if let Some(slot) = theme.bars.get_mut(n - FIXED_ROWS) {
                    *slot = rgb;
                }
            }
            _ => {}
        }
    }

    fn move_to(&mut self, row: usize) {
        self.selected = row.min(self.rows() - 1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorOutcome {
        if let Some(name) = &mut self.naming {
            match key.code {
                KeyCode::Esc => {
                    self.naming = None;
                    self.error = None;
                }
                KeyCode::Enter => {
                    if valid_name(name) {
                        return EditorOutcome::Save(name.trim().to_string());
                    }
                    self.error = Some("Use letters, numbers, spaces, - or _ (max 32)".to_string());
                }
                KeyCode::Backspace => {
                    name.pop();
                }
                KeyCode::Char(c) if name.chars().count() < 32 => name.push(c),
                _ => {}
            }
            return EditorOutcome::Stay;
        }

        if let Some(editing) = &mut self.color {
            let row = self.selected;
            match editing.picker.handle_key(key) {
                PickKey::Moved => {
                    let value = editing.picker.rgb();
                    self.set(row, Some(value));
                }
                PickKey::Done => self.color = None,
                PickKey::Cancel => {
                    let before = editing.before;
                    self.color = None;
                    self.set(row, before);
                }
                PickKey::Hex => {
                    let before = editing.before;
                    let current = editing.picker.rgb();
                    self.color = None;
                    self.set(row, before);
                    self.input = Some(to_hex(current));
                }
            }
            return EditorOutcome::Stay;
        }

        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Esc => {
                    self.input = None;
                    self.error = None;
                }
                KeyCode::Enter => match parse_hex(input) {
                    Some(value) => {
                        let row = self.selected;
                        self.input = None;
                        self.error = None;
                        self.set(row, Some(value));
                    }
                    None => self.error = Some("Enter a color like #ff5a4e".to_string()),
                },
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) if (c.is_ascii_hexdigit() || c == '#') && input.len() < 7 => {
                    input.push(c);
                }
                _ => {}
            }
            return EditorOutcome::Stay;
        }

        self.error = None;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return EditorOutcome::Cancel,
            KeyCode::Up | KeyCode::Char('k') => self.move_to(self.selected.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.move_to(self.selected + 1),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0),
            KeyCode::End | KeyCode::Char('G') => self.move_to(self.rows() - 1),
            KeyCode::Enter | KeyCode::Char('e') => {
                let before = self.get(self.selected);
                let start = before.unwrap_or(if self.selected == 0 {
                    (20, 20, 30)
                } else {
                    rgb(self.draft.accent)
                });
                self.color = Some(EditingColor {
                    picker: ColorPicker::from_rgb(start),
                    before,
                });
            }
            KeyCode::Char('#') => {
                let current = self
                    .get(self.selected)
                    .map(to_hex)
                    .unwrap_or_else(|| "#".to_string());
                self.input = Some(current);
            }
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                if Self::optional(self.selected) {
                    self.set(self.selected, None);
                } else {
                    self.error = Some("Only background and art can be cleared".to_string());
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.draft.bars.len() < MAX_BARS {
                    let last = self.draft.bars.last().copied().unwrap_or((255, 255, 255));
                    self.draft.bars.push(last);
                    self.selected = self.rows() - 1;
                } else {
                    self.error = Some(format!("At most {MAX_BARS} bar colors"));
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                if self.selected >= FIXED_ROWS && self.draft.bars.len() > MIN_BARS {
                    self.draft.bars.remove(self.selected - FIXED_ROWS);
                    self.move_to(self.selected);
                } else if self.selected < FIXED_ROWS {
                    self.error = Some("Select a bar color to remove it".to_string());
                } else {
                    self.error = Some(format!("At least {MIN_BARS} bar colors"));
                }
            }
            KeyCode::Char('s') => self.naming = Some(self.name.clone()),
            _ => {}
        }
        EditorOutcome::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin_themes;
    use crossterm::event::KeyModifiers;
    use ratatui::style::Color;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(editor: &mut Editor, text: &str) {
        for c in text.chars() {
            editor.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn editor() -> Editor {
        let base = builtin_themes().remove(1);
        Editor::new(&base, "mine".to_string(), 1)
    }

    #[test]
    fn hex_input() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Char('#')));
        for _ in 0..7 {
            editor.handle_key(key(KeyCode::Backspace));
        }
        type_text(&mut editor, "#102030");
        editor.handle_key(key(KeyCode::Enter));
        assert_eq!(editor.draft.accent, Color::Rgb(0x10, 0x20, 0x30));
        assert!(editor.input.is_none());
    }

    #[test]
    fn picker_live_cancel() {
        let mut editor = editor();
        let original = editor.draft.accent;
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.color.is_some());
        for _ in 0..5 {
            editor.handle_key(key(KeyCode::Right));
        }
        assert_ne!(editor.draft.accent, original);
        editor.handle_key(key(KeyCode::Esc));
        assert!(editor.color.is_none());
        assert_eq!(editor.draft.accent, original);

        editor.handle_key(key(KeyCode::Enter));
        for _ in 0..5 {
            editor.handle_key(key(KeyCode::Right));
        }
        let changed = editor.draft.accent;
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.color.is_none());
        assert_eq!(editor.draft.accent, changed);
    }

    #[test]
    fn picker_hash_hex() {
        let mut editor = editor();
        let original = editor.draft.accent;
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Char('#')));
        assert!(editor.color.is_none());
        assert!(editor.input.is_some());
        assert_eq!(editor.draft.accent, original);
    }

    #[test]
    fn picker_empty_row() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Home));
        editor.handle_key(key(KeyCode::Delete));
        assert!(editor.draft.background.is_none());
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Up));
        assert!(editor.draft.background.is_some());
        editor.handle_key(key(KeyCode::Esc));
        assert!(editor.draft.background.is_none());
    }

    #[test]
    fn picker_hue_wrap() {
        let mut picker = ColorPicker::from_rgb((255, 0, 0));
        picker.handle_key(key(KeyCode::Left));
        assert!(picker.h > 350.0);
        for _ in 0..100 {
            picker.handle_key(key(KeyCode::Up));
            picker.handle_key(key(KeyCode::PageUp));
        }
        assert!(picker.l <= 1.0 && picker.s <= 1.0);
    }

    #[test]
    fn generator_modes() {
        let mut generator = Generator::new((255, 90, 78), true, 0);
        let first = generator.draft.accent;
        assert!(generator.draft.background.is_some());
        generator.handle_key(key(KeyCode::Right));
        assert_ne!(generator.draft.accent, first);
        let dark_background = generator.draft.background;
        generator.handle_key(key(KeyCode::Char('m')));
        assert_ne!(generator.draft.background, dark_background);
        assert!(matches!(
            generator.handle_key(key(KeyCode::Enter)),
            GeneratorOutcome::Accept
        ));
        assert!(matches!(
            generator.handle_key(key(KeyCode::Esc)),
            GeneratorOutcome::Cancel
        ));
    }

    #[test]
    fn bad_hex() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Char('#')));
        for _ in 0..7 {
            editor.handle_key(key(KeyCode::Backspace));
        }
        type_text(&mut editor, "#12");
        editor.handle_key(key(KeyCode::Enter));
        assert!(editor.input.is_some());
        assert!(editor.error.is_some());
    }

    #[test]
    fn clear_rows() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Home));
        assert!(editor.draft.background.is_some());
        editor.handle_key(key(KeyCode::Delete));
        assert!(editor.draft.background.is_none());
        editor.handle_key(key(KeyCode::Down));
        editor.handle_key(key(KeyCode::Delete));
        assert!(editor.error.is_some());
        assert!(editor.get(1).is_some());
    }

    #[test]
    fn bars_limits() {
        let mut editor = editor();
        let start = editor.draft.bars.len();
        for _ in 0..10 {
            editor.handle_key(key(KeyCode::Char('+')));
        }
        assert_eq!(editor.draft.bars.len(), MAX_BARS);
        assert_eq!(editor.selected, editor.rows() - 1);
        for _ in 0..10 {
            editor.handle_key(key(KeyCode::Char('-')));
        }
        assert_eq!(editor.draft.bars.len(), MIN_BARS);
        assert!(start >= MIN_BARS);
        assert!(editor.selected < editor.rows());
    }

    #[test]
    fn save_name() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Char('s')));
        for _ in 0..10 {
            editor.handle_key(key(KeyCode::Backspace));
        }
        assert!(matches!(
            editor.handle_key(key(KeyCode::Enter)),
            EditorOutcome::Stay
        ));
        assert!(editor.error.is_some());
        type_text(&mut editor, "night shift");
        match editor.handle_key(key(KeyCode::Enter)) {
            EditorOutcome::Save(name) => assert_eq!(name, "night shift"),
            _ => panic!("expected save"),
        }
    }

    #[test]
    fn name_input_keys() {
        let mut editor = editor();
        editor.handle_key(key(KeyCode::Char('s')));
        type_text(&mut editor, "qs");
        assert!(editor.naming.as_deref().unwrap().ends_with("qs"));
        assert!(matches!(
            editor.handle_key(key(KeyCode::Esc)),
            EditorOutcome::Stay
        ));
        assert!(editor.naming.is_none());
    }

    #[test]
    fn picker_delete_needs_confirmation() {
        let mut picker = ThemePicker::new(3);
        assert!(matches!(
            picker.handle_key(key(KeyCode::Char('x')), 5),
            PickerOutcome::Delete
        ));
        picker.confirm = true;
        assert!(matches!(
            picker.handle_key(key(KeyCode::Char('n')), 5),
            PickerOutcome::Stay
        ));
        assert!(!picker.confirm);
        picker.confirm = true;
        assert!(matches!(
            picker.handle_key(key(KeyCode::Char('y')), 5),
            PickerOutcome::ConfirmDelete
        ));
        assert_eq!(picker.selected(), 3);
    }

    #[test]
    fn picker_bounds() {
        let mut picker = ThemePicker::new(0);
        picker.handle_key(key(KeyCode::Up), 5);
        assert_eq!(picker.selected(), 0);
        picker.handle_key(key(KeyCode::End), 5);
        assert_eq!(picker.selected(), 4);
        picker.handle_key(key(KeyCode::Down), 5);
        assert_eq!(picker.selected(), 4);
        assert!(matches!(
            picker.handle_key(key(KeyCode::Enter), 5),
            PickerOutcome::Apply
        ));
    }
}
