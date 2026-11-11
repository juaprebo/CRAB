use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SeekBy(i64),
    SetPosition(Duration),
    Quit,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: Option<PathBuf>,
    pub duration: Option<Duration>,
    pub playing: bool,
    pub stopped: bool,
    pub position: Duration,
}

impl Info {
    fn same_track(&self, other: &Info) -> bool {
        self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.cover == other.cover
            && self.duration == other.duration
    }

    fn same_state(&self, other: &Info) -> bool {
        self.playing == other.playing && self.stopped == other.stopped
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Command, Info};
    use souvlaki::{
        MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
        PlatformConfig, SeekDirection,
    };
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    pub struct Media {
        controls: MediaControls,
        rx: Receiver<Command>,
        last: Option<Info>,
        pushed_at: Instant,
    }

    fn translate(event: MediaControlEvent) -> Option<Command> {
        Some(match event {
            MediaControlEvent::Play => Command::Play,
            MediaControlEvent::Pause => Command::Pause,
            MediaControlEvent::Toggle => Command::Toggle,
            MediaControlEvent::Next => Command::Next,
            MediaControlEvent::Previous => Command::Previous,
            MediaControlEvent::Stop => Command::Stop,
            MediaControlEvent::Seek(SeekDirection::Forward) => Command::SeekBy(5),
            MediaControlEvent::Seek(SeekDirection::Backward) => Command::SeekBy(-5),
            MediaControlEvent::SeekBy(direction, amount) => {
                let secs = amount.as_secs() as i64;
                Command::SeekBy(match direction {
                    SeekDirection::Forward => secs,
                    SeekDirection::Backward => -secs,
                })
            }
            MediaControlEvent::SetPosition(MediaPosition(position)) => {
                Command::SetPosition(position)
            }
            MediaControlEvent::Quit => Command::Quit,
            _ => return None,
        })
    }

    impl Media {
        pub fn start() -> Option<Media> {
            let mut controls = MediaControls::new(PlatformConfig {
                dbus_name: "crab",
                display_name: "CRAB",
                hwnd: None,
            })
            .ok()?;
            let (tx, rx) = channel();
            controls
                .attach(move |event| {
                    if let Some(command) = translate(event) {
                        let _ = tx.send(command);
                    }
                })
                .ok()?;
            Some(Media {
                controls,
                rx,
                last: None,
                pushed_at: Instant::now(),
            })
        }

        pub fn poll(&self) -> Vec<Command> {
            let mut commands = Vec::new();
            while let Ok(command) = self.rx.try_recv() {
                commands.push(command);
            }
            commands
        }

        pub fn update(&mut self, info: &Info) {
            let (track_changed, state_changed) = match &self.last {
                Some(last) => (!last.same_track(info), !last.same_state(info)),
                None => (true, true),
            };
            let stale = self.pushed_at.elapsed() >= Duration::from_secs(5) && info.playing;
            if track_changed {
                let cover_url = info
                    .cover
                    .as_ref()
                    .map(|path| format!("file://{}", path.display()));
                let _ = self.controls.set_metadata(MediaMetadata {
                    title: Some(&info.title)
                        .filter(|t| !t.is_empty())
                        .map(|t| t.as_str()),
                    artist: Some(&info.artist)
                        .filter(|t| !t.is_empty())
                        .map(|t| t.as_str()),
                    album: Some(&info.album)
                        .filter(|t| !t.is_empty())
                        .map(|t| t.as_str()),
                    cover_url: cover_url.as_deref(),
                    duration: info.duration,
                });
            }
            if track_changed || state_changed || stale {
                let progress = Some(MediaPosition(info.position));
                let playback = if info.stopped {
                    MediaPlayback::Stopped
                } else if info.playing {
                    MediaPlayback::Playing { progress }
                } else {
                    MediaPlayback::Paused { progress }
                };
                let _ = self.controls.set_playback(playback);
                self.pushed_at = Instant::now();
            }
            self.last = Some(info.clone());
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{Command, Info};

    pub struct Media;

    impl Media {
        pub fn start() -> Option<Media> {
            None
        }

        pub fn poll(&self) -> Vec<Command> {
            Vec::new()
        }

        pub fn update(&mut self, _info: &Info) {}
    }
}

pub use platform::Media;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_detection() {
        let a = Info {
            title: "A".into(),
            playing: true,
            ..Info::default()
        };
        let mut b = a.clone();
        assert!(a.same_track(&b) && a.same_state(&b));
        b.position = Duration::from_secs(40);
        assert!(a.same_track(&b) && a.same_state(&b));
        b.playing = false;
        assert!(a.same_track(&b) && !a.same_state(&b));
        b.title = "B".into();
        assert!(!a.same_track(&b));
    }
}
