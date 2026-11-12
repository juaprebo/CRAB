use super::*;

impl App {
    pub(super) fn set_language(&mut self, language: Language) {
        if let Err(error) = self
            .language_service
            .execute(&mut self.config.language, language)
        {
            self.set_status(
                tr_message(
                    self.config.language,
                    "Language not saved: {error}",
                    &[("error", format!("{error:#}"))],
                ),
                true,
            );
            return;
        }
        self.set_status(
            tr_message(
                language,
                "Language: {name}",
                &[("name", language.name().into())],
            ),
            false,
        );
    }

    pub(super) fn language_picker_key(&mut self, key: KeyEvent) {
        let Some(index) = self.language_picker else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.language_picker = None,
            KeyCode::Up | KeyCode::Char('k') => {
                self.language_picker = Some((index + Language::ALL.len() - 1) % Language::ALL.len())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.language_picker = Some((index + 1) % Language::ALL.len())
            }
            KeyCode::Enter => {
                self.set_language(Language::ALL[index]);
                if !self.status_is_error {
                    self.language_picker = None;
                }
            }
            _ => {}
        }
    }
}
