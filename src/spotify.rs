use anyhow::{Context, Result, anyhow};
use librespot_connect::{ConnectConfig, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::{DeviceType, SessionConfig};
use librespot_core::session::Session;
use librespot_metadata::audio::UniqueFields;
use librespot_oauth::OAuthClientBuilder;
use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::config::PlayerConfig;
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::mixer::softmixer::SoftMixer;
use librespot_playback::mixer::{Mixer, MixerConfig};
use librespot_playback::player::{Player, PlayerEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, channel};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

pub const ENABLED: bool = true;

const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const DEVICE_NAME: &str = "CRAB";

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
    },
    Playing(u32),
    Paused(u32),
    Position(u32),
    Stopped,
}

pub struct SpotifyHandle {
    commands: UnboundedSender<SpotifyCommand>,
    pub events: Receiver<SpotifyEvent>,
}

impl SpotifyHandle {
    pub fn send(&self, command: SpotifyCommand) {
        let _ = self.commands.send(command);
    }
}

fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("crab").join("spotify"))
        .ok_or_else(|| anyhow!("no cache directory available"))
}

fn open_cache() -> Result<Cache> {
    let dir = cache_dir()?;
    Cache::new(Some(dir), None, None, None)
        .map_err(|e| anyhow!("cannot open credential cache: {e}"))
}

pub fn has_credentials() -> bool {
    open_cache()
        .map(|c| c.credentials().is_some())
        .unwrap_or(false)
}

pub fn login() -> Result<()> {
    let client_id = SessionConfig::default().client_id;
    let client = OAuthClientBuilder::new(&client_id, REDIRECT_URI, vec!["streaming"])
        .open_in_browser()
        .with_custom_message(
            "CRAB: Spotify login complete, you can close this tab and return to the terminal.",
        )
        .build()
        .map_err(|e| anyhow!("oauth setup failed: {e}"))?;
    let token = client
        .get_access_token()
        .map_err(|e| anyhow!("oauth login failed: {e}"))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let cache = open_cache()?;
        let session = Session::new(SessionConfig::default(), Some(cache));
        session
            .connect(Credentials::with_access_token(token.access_token), true)
            .await
            .map_err(|e| anyhow!("spotify session failed: {e}"))?;
        Ok::<(), anyhow::Error>(())
    })
}

struct ChannelSink {
    tx: SyncSender<Vec<f32>>,
}

impl Sink for ChannelSink {
    fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
        if let AudioPacket::Samples(samples) = packet {
            let chunk: Vec<f32> = samples.iter().map(|s| *s as f32).collect();
            self.tx
                .send(chunk)
                .map_err(|_| SinkError::OnWrite("audio engine closed".to_string()))?;
        }
        Ok(())
    }
}

pub fn start(audio: SyncSender<Vec<f32>>) -> SpotifyHandle {
    let (event_tx, event_rx) = channel::<SpotifyEvent>();
    let (command_tx, command_rx) = unbounded_channel::<SpotifyCommand>();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = event_tx.send(SpotifyEvent::Failed(e.to_string()));
                return;
            }
        };
        let failure_tx = event_tx.clone();
        if let Err(e) = runtime.block_on(run(audio, event_tx, command_rx)) {
            let _ = failure_tx.send(SpotifyEvent::Failed(format!("{e:#}")));
        }
    });

    SpotifyHandle {
        commands: command_tx,
        events: event_rx,
    }
}

async fn run(
    audio: SyncSender<Vec<f32>>,
    events: std::sync::mpsc::Sender<SpotifyEvent>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<SpotifyCommand>,
) -> Result<()> {
    let cache = open_cache()?;
    let credentials = cache
        .credentials()
        .context("no saved spotify credentials, run `crab login` first")?;
    let session = Session::new(SessionConfig::default(), Some(cache));
    let mixer = Arc::new(
        SoftMixer::open(MixerConfig::default()).map_err(|e| anyhow!("mixer setup failed: {e}"))?,
    );
    let player_config = PlayerConfig {
        position_update_interval: Some(Duration::from_millis(500)),
        ..Default::default()
    };
    let player = Player::new(
        player_config,
        session.clone(),
        mixer.get_soft_volume(),
        move || Box::new(ChannelSink { tx: audio }),
    );
    let mut player_events = player.get_player_event_channel();

    let connect_config = ConnectConfig {
        name: DEVICE_NAME.to_string(),
        device_type: DeviceType::Computer,
        ..Default::default()
    };
    let (spirc, spirc_task) = Spirc::new(connect_config, session, credentials, player, mixer)
        .await
        .map_err(|e| anyhow!("spotify connect failed: {e}"))?;
    let _ = events.send(SpotifyEvent::Ready);
    let mut spirc_task = Box::pin(spirc_task);

    loop {
        tokio::select! {
            _ = &mut spirc_task => {
                return Err(anyhow!("spotify connection closed"));
            }
            command = commands.recv() => {
                match command {
                    Some(SpotifyCommand::PlayPause) => { let _ = spirc.play_pause(); }
                    Some(SpotifyCommand::Next) => { let _ = spirc.next(); }
                    Some(SpotifyCommand::Previous) => { let _ = spirc.prev(); }
                    Some(SpotifyCommand::Shutdown) | None => {
                        let _ = spirc.shutdown();
                        return Ok(());
                    }
                }
            }
            event = player_events.recv() => {
                let Some(event) = event else { return Ok(()); };
                match event {
                    PlayerEvent::TrackChanged { audio_item } => {
                        let (artist, album) = match &audio_item.unique_fields {
                            UniqueFields::Track { artists, album, .. } => (
                                artists.0.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "),
                                album.clone(),
                            ),
                            UniqueFields::Local { artists, album, .. } => (
                                artists.clone().unwrap_or_default(),
                                album.clone().unwrap_or_default(),
                            ),
                            UniqueFields::Episode { show_name, .. } => (show_name.clone(), String::new()),
                        };
                        let _ = events.send(SpotifyEvent::Track {
                            title: audio_item.name.clone(),
                            artist,
                            album,
                            duration_ms: audio_item.duration_ms,
                        });
                    }
                    PlayerEvent::Playing { position_ms, .. } => {
                        let _ = events.send(SpotifyEvent::Playing(position_ms));
                    }
                    PlayerEvent::Paused { position_ms, .. } => {
                        let _ = events.send(SpotifyEvent::Paused(position_ms));
                    }
                    PlayerEvent::PositionChanged { position_ms, .. }
                    | PlayerEvent::PositionCorrection { position_ms, .. }
                    | PlayerEvent::Seeked { position_ms, .. } => {
                        let _ = events.send(SpotifyEvent::Position(position_ms));
                    }
                    PlayerEvent::Stopped { .. } | PlayerEvent::EndOfTrack { .. } => {
                        let _ = events.send(SpotifyEvent::Stopped);
                    }
                    _ => {}
                }
            }
        }
    }
}
