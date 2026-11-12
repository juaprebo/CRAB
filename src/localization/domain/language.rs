use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Es,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::En, Self::Es];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Self::En),
            "es" | "español" | "spanish" => Some(Self::Es),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Es => "Español",
        }
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_languages_and_rejects_unknown_selections() {
        assert_eq!(Language::parse(" ES "), Some(Language::Es));
        assert_eq!(Language::parse("español"), Some(Language::Es));
        assert_eq!(Language::parse("English"), Some(Language::En));
        assert_eq!(Language::parse("fr"), None);
    }
}
