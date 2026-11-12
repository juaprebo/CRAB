use super::language::Language;
use anyhow::Result;

pub trait LanguageRepository {
    fn save(&self, language: Language) -> Result<()>;
}
