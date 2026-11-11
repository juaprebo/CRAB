use crate::keys;
use std::time::Duration;

pub const COMMANDS: &[&str] = &[
    "quit", "help", "sort", "sleep", "eq", "repeat", "shuffle", "volume", "lyrics", "theme", "viz",
    "clock", "reload", "folder", "import", "export", "save", "tab", "play", "pause", "next",
    "prev", "stop", "seek", "queue", "map", "unmap", "reset", "keys", "stats",
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

pub fn complete(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    if !trimmed.contains(char::is_whitespace) {
        let matches: Vec<&str> = COMMANDS
            .iter()
            .copied()
            .filter(|c| c.starts_with(trimmed))
            .collect();
        return match matches.as_slice() {
            [] => None,
            [only] => Some(format!("{only} ")),
            many => {
                let first = many[0];
                let common = (1..=first.len())
                    .rev()
                    .find(|n| many.iter().all(|m| m.starts_with(&first[..*n])))
                    .unwrap_or(0);
                (common > trimmed.len()).then(|| first[..common].to_string())
            }
        };
    }
    let (name, args) = split(trimmed);
    let name = canonical(&name)?;
    if !matches!(name, "map" | "unmap" | "reset") || args.len() != 1 || trimmed.ends_with(' ') {
        return None;
    }
    let prefix = &args[0];
    let matches: Vec<&str> = keys::TABLE
        .iter()
        .map(|(_, n, _, _)| *n)
        .filter(|n| n.starts_with(prefix.as_str()))
        .collect();
    match matches.as_slice() {
        [only] => Some(format!("{name} {only} ")),
        _ => None,
    }
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
    fn completes_commands_and_actions() {
        assert_eq!(complete("sl").as_deref(), Some("sleep "));
        assert_eq!(complete("s").as_deref(), None);
        assert_eq!(complete("map sle").as_deref(), Some("map sleep "));
        assert_eq!(complete("xyz"), None);
        assert_eq!(complete("re").as_deref(), None);
        assert_eq!(complete("rep").as_deref(), Some("repeat "));
        assert_eq!(complete("sta").as_deref(), Some("stats "));
        assert_eq!(complete("ke").as_deref(), Some("keys "));
        assert_eq!(complete("reset slee").as_deref(), Some("reset sleep "));
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
