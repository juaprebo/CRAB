pub fn country_from_locale(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or("");
    match base.split_once('_') {
        Some((_, region))
            if region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()) =>
        {
            region.to_ascii_uppercase()
        }
        _ => String::new(),
    }
}

pub fn default_country() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let country = country_from_locale(&value);
            if !country.is_empty() {
                return country;
            }
        }
    }
    String::new()
}
