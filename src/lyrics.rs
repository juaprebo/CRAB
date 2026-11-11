use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    pub lines: Vec<(Option<Duration>, String)>,
}

impl Lyrics {
    pub fn synced(&self) -> bool {
        self.lines.iter().any(|(t, _)| t.is_some())
    }

    pub fn current(&self, position: Duration) -> Option<usize> {
        if !self.synced() {
            return None;
        }
        self.lines
            .iter()
            .rposition(|(t, _)| t.is_some_and(|t| t <= position))
    }
}

fn parse_stamp(text: &str) -> Option<Duration> {
    let (minutes, rest) = text.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let seconds: f64 = rest.trim().parse().ok()?;
    if !(0.0..600.0).contains(&seconds) {
        return None;
    }
    Some(Duration::from_secs(minutes * 60) + Duration::from_secs_f64(seconds))
}

pub fn parse(text: &str) -> Lyrics {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim_end();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some((inside, after)) = stripped.split_once(']') else {
                break;
            };
            match parse_stamp(inside) {
                Some(stamp) => {
                    stamps.push(stamp);
                    rest = after;
                }
                None => break,
            }
        }
        if stamps.is_empty() {
            let is_tag = rest.starts_with('[')
                && rest
                    .split_once(']')
                    .is_some_and(|(k, _)| k.contains(':') && !k.contains(' '));
            if !is_tag {
                lines.push((None, rest.to_string()));
            }
        } else {
            let line = rest.trim().to_string();
            for stamp in stamps {
                lines.push((Some(stamp), line.clone()));
            }
        }
    }
    if lines.iter().any(|(t, _)| t.is_some()) {
        lines.retain(|(t, _)| t.is_some());
        lines.sort_by_key(|(t, _)| *t);
    }
    while lines.last().is_some_and(|(_, l)| l.is_empty()) {
        lines.pop();
    }
    Lyrics { lines }
}

pub fn load(path: &Path) -> Option<Lyrics> {
    let sidecar = path.with_extension("lrc");
    if let Ok(text) = std::fs::read_to_string(&sidecar) {
        let lyrics = parse(&text);
        if !lyrics.lines.is_empty() {
            return Some(lyrics);
        }
    }
    let plain = path.with_extension("txt");
    if let Ok(text) = std::fs::read_to_string(&plain) {
        let lyrics = parse(&text);
        if !lyrics.lines.is_empty() {
            return Some(lyrics);
        }
    }
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let text = tagged
        .primary_tag()
        .into_iter()
        .chain(tagged.tags().iter())
        .find_map(|tag| tag.get_string(ItemKey::Lyrics).map(str::to_string))?;
    let lyrics = parse(&text);
    (!lyrics.lines.is_empty()).then_some(lyrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lines() {
        let lyrics =
            parse("[ar:Someone]\n[00:12.50]First\n[01:02.00][01:30.25]Chorus\n\n[00:05]Intro");
        assert!(lyrics.synced());
        let texts: Vec<_> = lyrics.lines.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(texts, ["Intro", "First", "Chorus", "Chorus"]);
        assert_eq!(lyrics.lines[1].0, Some(Duration::from_millis(12_500)));
        assert_eq!(lyrics.current(Duration::from_secs(3)), None);
        assert_eq!(lyrics.current(Duration::from_secs(13)), Some(1));
        assert_eq!(lyrics.current(Duration::from_secs(100)), Some(3));
    }

    #[test]
    fn plain_text_is_unsynced() {
        let lyrics = parse("one\ntwo\n\nthree\n");
        assert!(!lyrics.synced());
        assert_eq!(lyrics.lines.len(), 4);
        assert_eq!(lyrics.current(Duration::from_secs(5)), None);
    }

    #[test]
    fn sidecar_is_loaded() {
        let dir = std::env::temp_dir().join(format!("crab-lrc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("song.wav");
        crate::testutil::write_wav(&song, 1);
        std::fs::write(dir.join("song.lrc"), "[00:01.00]Hello\n").unwrap();
        let lyrics = load(&song).unwrap();
        assert_eq!(lyrics.lines.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
