use crate::localization::domain::language::Language;
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub(crate) static SPANISH: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../locales/es.json")).expect("valid Spanish catalog")
});

pub fn lookup(language: Language, english: &str) -> &str {
    match language {
        Language::En => english,
        Language::Es => SPANISH.get(english).map(String::as_str).unwrap_or(english),
    }
}
