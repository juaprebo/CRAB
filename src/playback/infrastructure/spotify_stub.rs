#![allow(dead_code)]

use anyhow::{Result, anyhow};
use std::sync::mpsc::{Receiver, SyncSender, channel};

pub const ENABLED: bool = false;

pub enum SpotifyCommand {
    PlayPause,
    Next,
    Previous,
    Shutdown,
}

pub enum SpotifyEvent {
    Ready,
    Failed(String),
    Track {
        title: String,
        artist: String,
        album: String,
        duration_ms: u32,
        cover_url: Option<String>,
    },
    Playing(u32),
    Paused(u32),
    Position(u32),
    Stopped,
}

pub struct SpotifyHandle {
    pub events: Receiver<SpotifyEvent>,
}

impl SpotifyHandle {
    pub fn send(&self, _command: SpotifyCommand) {}
}

pub fn has_credentials() -> bool {
    false
}

pub fn login() -> Result<()> {
    Err(anyhow!(
        "this build of CRAB was compiled without Spotify support"
    ))
}

pub fn start(_audio: SyncSender<Vec<f32>>) -> SpotifyHandle {
    let (_tx, events) = channel();
    SpotifyHandle { events }
}
