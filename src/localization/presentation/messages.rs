use crate::localization::domain::language::Language;
pub fn text(language: Language, english: &str) -> &str {
    crate::localization::infrastructure::catalog::lookup(language, english)
}

pub fn message(language: Language, english: &str, arguments: &[(&str, String)]) -> String {
    let template = text(language, english);
    let mut result = String::new();
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        result.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find('}').map(|end| start + end) else {
            result.push_str(&remaining[start..]);
            return result;
        };
        let key = &remaining[start + 1..end];
        if let Some((_, value)) = arguments.iter().find(|(name, _)| *name == key) {
            result.push_str(value);
        } else {
            result.push_str(&remaining[start..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    result.push_str(remaining);
    result
}

pub fn playlist_error(
    language: Language,
    error: &crate::playlists::domain::playlist::PlaylistError,
) -> String {
    use crate::playlists::domain::playlist::PlaylistError;
    match error {
        PlaylistError::EmptyName => text(language, "Playlist name cannot be empty").into(),
        PlaylistError::MissingPlaylist => text(language, "Playlist no longer exists").into(),
        PlaylistError::DuplicateName(name) => message(
            language,
            "A playlist named {name} already exists",
            &[("name", name.clone())],
        ),
    }
}

pub fn file_error(language: Language, error: &anyhow::Error) -> String {
    if let Some(error) = error.downcast_ref::<crate::library::domain::file_name::FileNameError>() {
        return text(language, &error.to_string()).into();
    }
    message(
        language,
        "Cannot rename file: {error}",
        &[("error", format!("{error:#}"))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::infrastructure::catalog::SPANISH;
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    fn placeholders(template: &str) -> BTreeSet<&str> {
        template
            .split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(key, _)| key))
            .collect()
    }

    #[test]
    fn catalogs_have_matching_keys_and_placeholders() {
        let english: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../../locales/en.json")).unwrap();
        assert_eq!(
            english.keys().collect::<Vec<_>>(),
            SPANISH.keys().collect::<Vec<_>>()
        );
        for (key, translation) in SPANISH.iter() {
            assert!(!translation.is_empty(), "{key}");
            assert_eq!(placeholders(key), placeholders(translation), "{key}");
            assert_eq!(&english[key], key);
        }
    }

    #[test]
    fn translation_preserves_arguments_and_falls_back_to_english() {
        assert_eq!(text(Language::Es, "Library"), "Biblioteca");
        assert_eq!(text(Language::En, "Library"), "Library");
        assert_eq!(
            text(Language::Es, "An untranslated diagnostic"),
            "An untranslated diagnostic"
        );
        assert_eq!(
            message(
                Language::Es,
                "Renamed playlist to {name}",
                &[("name", "Library {name} 日本語".into())]
            ),
            "Playlist renombrada a Library {name} 日本語"
        );
        assert_eq!(
            message(
                Language::Es,
                "Cannot read {arg0}: {error}",
                &[
                    ("arg0", "/music/Library.mp3".into()),
                    ("error", "{arg0} unavailable".into())
                ]
            ),
            "No se puede leer /music/Library.mp3: {arg0} unavailable"
        );
    }
}
