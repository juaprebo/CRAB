use crate::keys;
use std::time::Duration;

pub const COMMANDS: &[&str] = &[
    "quit", "help", "sort", "sleep", "eq", "repeat", "shuffle", "volume", "lyrics", "theme", "viz",
    "clock", "reload", "folder", "import", "export", "save", "tab", "play", "pause", "next",
    "prev", "stop", "seek", "queue", "map", "unmap", "reset", "keys", "stats", "fav", "favs",
    "goto", "find", "add", "mute", "fade",
];

pub fn split(line: &str) -> (String, Vec<String>) {
    let mut parts = line.split_whitespace().map(str::to_string);
    let name = parts.next().unwrap_or_default();
    (name, parts.collect())
}

pub fn canonical(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "q" | "exit" => "quit",
        "vol" => "volume",
        "visualizer" => "viz",
        "previous" => "prev",
        "w" => "save",
        _ => lower.as_str(),
    };
    COMMANDS.iter().copied().find(|c| *c == alias)
}

fn values(name: &str, themes: &[String]) -> Vec<String> {
    let words = |list: &[&str]| list.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    match name {
        "sort" => words(&["path", "title", "artist", "album"]),
        "eq" => crate::eq::PRESETS
            .iter()
            .map(|(n, _)| n.to_string())
            .collect(),
        "repeat" => words(&["off", "all", "one"]),
        "shuffle" => words(&["on", "off"]),
        "viz" => crate::config::VizStyle::ALL
            .iter()
            .map(|s| s.name().to_string())
            .collect(),
        "keys" => words(&["crab", "vim", "custom"]),
        "tab" => words(&["library", "playlists", "radio", "spotify"]),
        "sleep" | "fade" => words(&["off"]),
        "theme" => themes.to_vec(),
        "map" | "unmap" | "reset" => keys::TABLE
            .iter()
            .map(|(_, n, _, _)| n.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn candidates(input: &str, themes: &[String]) -> (String, Vec<String>) {
    let trimmed = input.trim_start();
    let lead = &input[..input.len() - trimmed.len()];
    let Some(space) = trimmed.find(char::is_whitespace) else {
        let list = COMMANDS
            .iter()
            .filter(|c| c.starts_with(trimmed))
            .map(|c| c.to_string())
            .collect();
        return (lead.to_string(), list);
    };
    let (head, rest) = trimmed.split_at(space);
    let Some(name) = canonical(head) else {
        return (String::new(), Vec::new());
    };
    let rest_trimmed = rest.trim_start();
    if rest_trimmed.contains(char::is_whitespace) {
        return (String::new(), Vec::new());
    }
    let list = values(name, themes)
        .into_iter()
        .filter(|v| v.to_lowercase().starts_with(&rest_trimmed.to_lowercase()))
        .collect();
    let base = format!("{lead}{head}{}", &rest[..rest.len() - rest_trimmed.len()]);
    (base, list)
}

pub fn common_prefix(list: &[String]) -> String {
    let Some(first) = list.first() else {
        return String::new();
    };
    let mut end = first.len();
    for item in &list[1..] {
        end = end.min(
            first
                .chars()
                .zip(item.chars())
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum(),
        );
    }
    first[..end].to_string()
}

#[derive(Debug, PartialEq)]
pub enum SeekTo {
    Absolute(Duration),
    Relative(i64),
}

pub fn parse_seek(arg: &str) -> Option<SeekTo> {
    let (sign, rest) = match arg.chars().next()? {
        '+' => (Some(1i64), &arg[1..]),
        '-' => (Some(-1i64), &arg[1..]),
        _ => (None, arg),
    };
    let secs = match rest.split_once(':') {
        Some((m, s)) => m.parse::<u64>().ok()? * 60 + s.parse::<u64>().ok()?,
        None => rest.parse::<u64>().ok()?,
    };
    match sign {
        Some(sign) => Some(SeekTo::Relative(sign * secs as i64)),
        None => Some(SeekTo::Absolute(Duration::from_secs(secs))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_resolves_aliases() {
        let (name, args) = split("  vol   80 ");
        assert_eq!(canonical(&name), Some("volume"));
        assert_eq!(args, ["80"]);
        assert_eq!(canonical("Q"), Some("quit"));
        assert_eq!(canonical("nope"), None);
    }

    #[test]
    fn completes_commands_values_and_actions() {
        let none: Vec<String> = Vec::new();
        let (base, list) = candidates("sl", &none);
        assert_eq!(
            (base.as_str(), list.as_slice()),
            ("", &["sleep".to_string()][..])
        );
        assert!(candidates("s", &none).1.len() > 1);
        assert!(candidates("xyz", &none).1.is_empty());
        let (base, list) = candidates("map sle", &none);
        assert_eq!(base, "map ");
        assert_eq!(list, ["sleep"]);
        let (base, list) = candidates("eq ro", &none);
        assert_eq!(base, "eq ");
        assert_eq!(list, ["rock"]);
        assert_eq!(candidates("sort ", &none).1.len(), 4);
        assert_eq!(candidates("vol 5", &none).1.len(), 0);
        let themes = vec!["synthwave".to_string(), "solar".to_string()];
        assert_eq!(candidates("theme s", &themes).1.len(), 2);
        assert_eq!(common_prefix(&themes), "s");
    }

    #[test]
    fn parses_seek_targets() {
        assert_eq!(
            parse_seek("1:30"),
            Some(SeekTo::Absolute(Duration::from_secs(90)))
        );
        assert_eq!(
            parse_seek("45"),
            Some(SeekTo::Absolute(Duration::from_secs(45)))
        );
        assert_eq!(parse_seek("+10"), Some(SeekTo::Relative(10)));
        assert_eq!(parse_seek("-5"), Some(SeekTo::Relative(-5)));
        assert_eq!(parse_seek("x"), None);
    }
}
