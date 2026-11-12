use crate::localization::domain::{language::Language, language_repository::LanguageRepository};
use anyhow::Result;

pub struct ConfigLanguageRepository;

impl LanguageRepository for ConfigLanguageRepository {
    fn save(&self, language: Language) -> Result<()> {
        crate::settings::infrastructure::config::pick_language(language)
    }
}
