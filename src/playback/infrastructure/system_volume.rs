use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub volume: f32,
    pub muted: bool,
}

pub enum Request {
    Set(f32),
    ToggleMute,
}

#[derive(Clone, Copy)]
enum Backend {
    Wpctl,
    Pactl,
    Amixer,
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_quiet(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn parse_wpctl(text: &str) -> Option<Reading> {
    let value = text.split_whitespace().nth(1)?.parse::<f32>().ok()?;
    Some(Reading {
        volume: value,
        muted: text.contains("[MUTED]"),
    })
}

fn parse_percent(text: &str) -> Option<f32> {
    text.split_whitespace()
        .find_map(|token| token.strip_suffix('%'))
        .and_then(|digits| digits.parse::<f32>().ok())
        .map(|percent| percent / 100.0)
}

fn parse_pactl(volume: &str, mute: &str) -> Option<Reading> {
    Some(Reading {
        volume: parse_percent(volume)?,
        muted: mute.contains("yes"),
    })
}

fn parse_amixer(text: &str) -> Option<Reading> {
    let start = text.rfind('[')?;
    let line_start = text[..start].rfind("Playback").unwrap_or(0);
    let section = &text[line_start..];
    let open = section.find('[')?;
    let close = section[open..].find(']')? + open;
    let percent = section[open + 1..close]
        .strip_suffix('%')?
        .parse::<f32>()
        .ok()?;
    Some(Reading {
        volume: percent / 100.0,
        muted: section.contains("[off]"),
    })
}

impl Backend {
    fn read(self) -> Option<Reading> {
        match self {
            Backend::Wpctl => parse_wpctl(&run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?),
            Backend::Pactl => {
                let volume = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?;
                let mute = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])?;
                parse_pactl(&volume, &mute)
            }
            Backend::Amixer => parse_amixer(&run("amixer", &["get", "Master"])?),
        }
    }

    fn set(self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        match self {
            Backend::Wpctl => run_quiet(
                "wpctl",
                &[
                    "set-volume",
                    "@DEFAULT_AUDIO_SINK@",
                    &format!("{volume:.2}"),
                ],
            ),
            Backend::Pactl => run_quiet(
                "pactl",
                &[
                    "set-sink-volume",
                    "@DEFAULT_SINK@",
                    &format!("{}%", (volume * 100.0).round() as u32),
                ],
            ),
            Backend::Amixer => run_quiet(
                "amixer",
                &[
                    "set",
                    "Master",
                    &format!("{}%", (volume * 100.0).round() as u32),
                ],
            ),
        }
    }

    fn toggle_mute(self) {
        match self {
            Backend::Wpctl => run_quiet("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]),
            Backend::Pactl => run_quiet("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "toggle"]),
            Backend::Amixer => run_quiet("amixer", &["set", "Master", "toggle"]),
        }
    }
}

pub struct SystemVolume {
    requests: Sender<Request>,
    pub readings: Receiver<Reading>,
    pub initial: Reading,
}

impl SystemVolume {
    pub fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}

pub fn start() -> Option<SystemVolume> {
    let (backend, initial) = [Backend::Wpctl, Backend::Pactl, Backend::Amixer]
        .into_iter()
        .find_map(|backend| backend.read().map(|reading| (backend, reading)))?;

    let (request_tx, request_rx) = channel::<Request>();
    let (reading_tx, reading_rx) = channel::<Reading>();

    std::thread::spawn(move || {
        let mut last = initial;
        loop {
            match request_rx.recv_timeout(Duration::from_millis(350)) {
                Ok(request) => {
                    let mut target = None;
                    let mut toggle = matches!(request, Request::ToggleMute);
                    if let Request::Set(value) = request {
                        target = Some(value);
                    }
                    while let Ok(next) = request_rx.try_recv() {
                        match next {
                            Request::Set(value) => target = Some(value),
                            Request::ToggleMute => toggle = !toggle,
                        }
                    }
                    if let Some(value) = target {
                        backend.set(value);
                    }
                    if toggle {
                        backend.toggle_mute();
                    }
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            if let Some(reading) = backend.read()
                && reading != last
            {
                last = reading;
                if reading_tx.send(reading).is_err() {
                    return;
                }
            }
        }
    });

    Some(SystemVolume {
        requests: request_tx,
        readings: reading_rx,
        initial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wpctl_case() {
        let reading = parse_wpctl("Volume: 0.45\n").unwrap();
        assert_eq!(
            reading,
            Reading {
                volume: 0.45,
                muted: false
            }
        );
        let muted = parse_wpctl("Volume: 0.30 [MUTED]\n").unwrap();
        assert!(muted.muted);
    }

    #[test]
    fn parse_pactl_case() {
        let volume = "Volume: front-left: 29491 /  45% / -20.85 dB,   front-right: 29491 /  45% / -20.85 dB\n";
        let reading = parse_pactl(volume, "Mute: no\n").unwrap();
        assert!((reading.volume - 0.45).abs() < 1e-6);
        assert!(!reading.muted);
        assert!(parse_pactl(volume, "Mute: yes\n").unwrap().muted);
    }

    #[test]
    fn parse_amixer_case() {
        let text = "Simple mixer control 'Master',0\n  Capabilities: pvolume pswitch\n  Front Left: Playback 39321 [60%] [on]\n  Front Right: Playback 39321 [60%] [on]\n";
        let reading = parse_amixer(text).unwrap();
        assert!((reading.volume - 0.6).abs() < 1e-6);
        assert!(!reading.muted);
    }
}
