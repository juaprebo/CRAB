use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::radio::application::search_stations_service::SearchStationsService;
use crate::radio::domain::station::Station;
use crate::radio::domain::station_search_repository::Found;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::sync::mpsc::{Receiver, TryRecvError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Name,
    Country,
    Results,
}

#[derive(Debug, PartialEq)]
pub enum FinderOutcome {
    None,
    Close,
    Play(Station),
}

pub struct Finder {
    search: SearchStationsService,
    pub language: Language,
    pub name: String,
    pub country: String,
    pub focus: Focus,
    pub results: Vec<Found>,
    pub state: ListState,
    pub searching: bool,
    pub message: String,
    pub message_is_error: bool,
    rx: Option<Receiver<Result<Vec<Found>, String>>>,
}

impl Finder {
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    pub fn new(country: String, search: SearchStationsService) -> Self {
        Self {
            language: Language::default(),
            search,
            name: String::new(),
            country,
            focus: Focus::Name,
            results: Vec::new(),
            state: ListState::default(),
            searching: false,
            message: String::new(),
            message_is_error: false,
            rx: None,
        }
    }

    pub fn start(&mut self) {
        if self.searching {
            return;
        }
        self.searching = true;
        self.message = tr(self.language, "Searching").to_string();
        self.message_is_error = false;
        self.rx = Some(self.search.execute(self.name.clone(), self.country.clone()));
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err(tr(self.language, "search stopped unexpectedly").to_string())
            }
        };
        self.rx = None;
        self.searching = false;
        match outcome {
            Ok(results) => {
                self.message_is_error = false;
                self.message = match results.len() {
                    0 => tr(self.language, "No playable stations found").to_string(),
                    1 => tr(self.language, "1 station").to_string(),
                    n => tr_message(
                        self.language,
                        "{count} stations",
                        &[("count", n.to_string())],
                    ),
                };
                self.state
                    .select(if results.is_empty() { None } else { Some(0) });
                if !results.is_empty() {
                    self.focus = Focus::Results;
                }
                self.results = results;
            }
            Err(error) => {
                self.message_is_error = true;
                self.message = tr_message(
                    self.language,
                    "Station search failed: {error}",
                    &[("error", error)],
                );
            }
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    fn cycle(&mut self, forward: bool) {
        let mut order = vec![Focus::Name, Focus::Country];
        if !self.results.is_empty() {
            order.push(Focus::Results);
        }
        let current = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward {
            (current + 1) % order.len()
        } else {
            (current + order.len() - 1) % order.len()
        };
        self.focus = order[next];
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.results.len();
        if len == 0 {
            return;
        }
        let current = self.state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        self.state.select(Some(next));
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FinderOutcome {
        match key.code {
            KeyCode::Esc => return FinderOutcome::Close,
            KeyCode::Tab => {
                self.cycle(true);
                return FinderOutcome::None;
            }
            KeyCode::BackTab => {
                self.cycle(false);
                return FinderOutcome::None;
            }
            _ => {}
        }
        match self.focus {
            Focus::Name | Focus::Country => self.field_key(key),
            Focus::Results => self.results_key(key),
        }
    }

    fn field_key(&mut self, key: KeyEvent) -> FinderOutcome {
        match key.code {
            KeyCode::Enter => self.start(),
            KeyCode::Down => {
                if !self.results.is_empty() {
                    self.focus = Focus::Results;
                }
            }
            KeyCode::Backspace => {
                if self.focus == Focus::Name {
                    self.name.pop();
                } else {
                    self.country.pop();
                }
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.focus == Focus::Name {
                    self.name.push(c);
                } else if c.is_ascii_alphabetic() && self.country.chars().count() < 2 {
                    self.country.push(c.to_ascii_uppercase());
                }
            }
            _ => {}
        }
        FinderOutcome::None
    }

    fn results_key(&mut self, key: KeyEvent) -> FinderOutcome {
        match key.code {
            KeyCode::Enter => {
                if let Some(found) = self.selected().and_then(|i| self.results.get(i)) {
                    return FinderOutcome::Play(found.station.clone());
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected().unwrap_or(0) == 0 {
                    self.focus = Focus::Name;
                } else {
                    self.move_selection(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home | KeyCode::Char('g') => self.move_selection(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.move_selection(isize::MAX / 2),
            KeyCode::Char('/') => self.focus = Focus::Name,
            _ => {}
        }
        FinderOutcome::None
    }
}

#[cfg(test)]
mod tests {
    use crate::radio::application::search_stations_service::test_service;
    use crate::radio::infrastructure::{
        locale::country_from_locale, search_repository::parse_results,
    };

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    const SAMPLE: &str = r#"[
        {"name":" Radio Uno ","url_resolved":"https://a.example/uno.mp3","url":"https://a.example/uno.pls","codec":"MP3","bitrate":128,"countrycode":"es","hls":0,"lastcheckok":1},
        {"name":"Radio Dos","url_resolved":"https://a.example/dos.aac","codec":"AAC+","bitrate":64,"countrycode":"ES","hls":0,"lastcheckok":1},
        {"name":"Segments","url_resolved":"https://a.example/seg.m3u8","codec":"AAC","bitrate":96,"countrycode":"ES","hls":1,"lastcheckok":1},
        {"name":"Broken","url_resolved":"https://a.example/broken.mp3","codec":"MP3","bitrate":96,"countrycode":"ES","hls":0,"lastcheckok":0},
        {"name":"Opus only","url_resolved":"https://a.example/opus","codec":"OPUS","bitrate":96,"countrycode":"ES","hls":0,"lastcheckok":1},
        {"name":"Duplicate","url_resolved":"https://a.example/uno.mp3","codec":"MP3","bitrate":128,"countrycode":"ES","hls":0,"lastcheckok":1},
        {"name":"","url_resolved":"https://a.example/noname.mp3","codec":"MP3","bitrate":128,"countrycode":"ES","hls":0,"lastcheckok":1},
        {"name":"Fallback url","url_resolved":"","url":"http://b.example/stream.ogg","codec":"OGG","bitrate":null,"countrycode":null,"hls":null,"lastcheckok":null},
        {"name":"Not web","url_resolved":"rtsp://c.example/x","codec":"MP3","bitrate":1,"countrycode":"ES","hls":0,"lastcheckok":1}
    ]"#;

    #[test]
    fn parses_and_filters() {
        let found = parse_results(SAMPLE).unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.station.name.as_str()).collect();
        assert_eq!(names, ["Radio Uno", "Radio Dos", "Fallback url"]);
        assert_eq!(found[0].country, "ES");
        assert_eq!(found[0].codec, "MP3");
        assert_eq!(found[0].bitrate, 128);
        assert_eq!(found[2].station.url, "http://b.example/stream.ogg");
        assert_eq!(found[2].bitrate, 0);
        assert!(parse_results("not json").is_err());
        assert!(parse_results("[]").unwrap().is_empty());
    }

    #[test]
    fn locale_country() {
        assert_eq!(country_from_locale("es_ES.UTF-8"), "ES");
        assert_eq!(country_from_locale("en_gb@euro"), "GB");
        assert_eq!(country_from_locale("C"), "");
        assert_eq!(country_from_locale("POSIX"), "");
        assert_eq!(country_from_locale("es"), "");
    }

    #[test]
    fn typing_and_focus() {
        let mut finder = Finder::new(String::new(), test_service());
        for c in "cadena".chars() {
            finder.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(finder.name, "cadena");
        finder.on_key(key(KeyCode::Backspace));
        assert_eq!(finder.name, "caden");
        finder.on_key(key(KeyCode::Tab));
        assert_eq!(finder.focus, Focus::Country);
        for c in "e5s1".chars() {
            finder.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(finder.country, "ES");
        finder.on_key(key(KeyCode::Tab));
        assert_eq!(finder.focus, Focus::Name);
        assert_eq!(finder.on_key(key(KeyCode::Esc)), FinderOutcome::Close);
    }

    #[test]
    fn results_navigation() {
        let mut finder = Finder::new("ES".to_string(), test_service());
        finder.results = parse_results(SAMPLE).unwrap();
        finder.state.select(Some(0));
        finder.focus = Focus::Results;
        finder.on_key(key(KeyCode::Down));
        finder.on_key(key(KeyCode::Down));
        finder.on_key(key(KeyCode::Down));
        assert_eq!(finder.selected(), Some(2));
        finder.on_key(key(KeyCode::Home));
        assert_eq!(finder.selected(), Some(0));
        finder.on_key(key(KeyCode::End));
        assert_eq!(finder.selected(), Some(2));
        match finder.on_key(key(KeyCode::Enter)) {
            FinderOutcome::Play(station) => assert_eq!(station.name, "Fallback url"),
            other => panic!("unexpected {other:?}"),
        }
        finder.focus = Focus::Results;
        finder.state.select(Some(0));
        finder.on_key(key(KeyCode::Up));
        assert_eq!(finder.focus, Focus::Name);
        finder.on_key(key(KeyCode::Down));
        assert_eq!(finder.focus, Focus::Results);
        finder.on_key(key(KeyCode::Char('/')));
        assert_eq!(finder.focus, Focus::Name);
    }

    #[test]
    fn tab_skips_empty_results() {
        let mut finder = Finder::new(String::new(), test_service());
        finder.on_key(key(KeyCode::Tab));
        finder.on_key(key(KeyCode::Tab));
        assert_eq!(finder.focus, Focus::Name);
        finder.on_key(key(KeyCode::BackTab));
        assert_eq!(finder.focus, Focus::Country);
    }
}
