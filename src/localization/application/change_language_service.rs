use crate::localization::domain::{language::Language, language_repository::LanguageRepository};
use anyhow::Result;

pub struct ChangeLanguageService {
    repository: Box<dyn LanguageRepository>,
}

impl ChangeLanguageService {
    pub fn new(repository: Box<dyn LanguageRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(&self, current: &mut Language, selected: Language) -> Result<()> {
        self.repository.save(selected)?;
        *current = selected;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Repository {
        fail: bool,
        saved: Rc<Cell<Language>>,
    }
    impl LanguageRepository for Repository {
        fn save(&self, language: Language) -> Result<()> {
            anyhow::ensure!(!self.fail, "settings unavailable");
            self.saved.set(language);
            Ok(())
        }
    }
    #[test]
    fn language_changes_only_after_the_repository_accepts_it() {
        let saved = Rc::new(Cell::new(Language::En));
        let mut current = Language::En;
        let failure = ChangeLanguageService::new(Box::new(Repository {
            fail: true,
            saved: saved.clone(),
        }));
        assert!(failure.execute(&mut current, Language::Es).is_err());
        assert_eq!(current, Language::En);
        assert_eq!(saved.get(), Language::En);
        let service = ChangeLanguageService::new(Box::new(Repository {
            fail: false,
            saved: saved.clone(),
        }));
        service.execute(&mut current, Language::Es).unwrap();
        assert_eq!(current, Language::Es);
        assert_eq!(saved.get(), Language::Es);
    }
}
