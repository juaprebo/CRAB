use crate::analyzer::SampleTap;
use crate::eq::{EqParams, Fx};
use anyhow::{Context, Result, anyhow};
use rodio::{
    ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Sample, SampleRate, Source,
};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub enum Input {
    File(BufReader<File>),
    Stream(Mutex<crate::radio::BufferedStream>),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Input::File(reader) => reader.read(buf),
            Input::Stream(stream) => stream.get_mut().unwrap().read(buf),
        }
    }
}

impl Seek for Input {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Input::File(reader) => reader.seek(pos),
            Input::Stream(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stream is not seekable",
            )),
        }
    }
}

pub struct Tapped<S: Source> {
    inner: S,
    tap: Arc<SampleTap>,
    channels: usize,
    frame_sum: f32,
    frame_fill: usize,
    batch: Vec<f32>,
}

impl<S: Source> Tapped<S> {
    pub fn new(inner: S, tap: Arc<SampleTap>) -> Self {
        let channels = inner.channels().get() as usize;
        tap.set_sample_rate(inner.sample_rate().get());
        Self {
            inner,
            tap,
            channels,
            frame_sum: 0.0,
            frame_fill: 0,
            batch: Vec::with_capacity(512),
        }
    }

    fn feed(&mut self, sample: f32) {
        self.frame_sum += sample;
        self.frame_fill += 1;
        if self.frame_fill >= self.channels {
            self.batch.push(self.frame_sum / self.channels as f32);
            self.frame_sum = 0.0;
            self.frame_fill = 0;
            if self.batch.len() >= 512 {
                self.tap.push(&self.batch);
                self.batch.clear();
            }
        }
    }
}

impl<S: Source> Iterator for Tapped<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let sample = self.inner.next()?;
        self.feed(sample);
        Some(sample)
    }
}

impl<S: Source> Source for Tapped<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.tap.clear();
        self.batch.clear();
        self.inner.try_seek(pos)
    }
}

struct SpotifySource {
    rx: Receiver<Vec<f32>>,
    chunk: Vec<f32>,
    index: usize,
    tap: Arc<SampleTap>,
    left: f32,
    has_left: bool,
    batch: Vec<f32>,
}

impl Iterator for SpotifySource {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.index >= self.chunk.len() {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.chunk = chunk;
                    self.index = 0;
                }
                Err(TryRecvError::Empty) => {
                    if !self.batch.is_empty() {
                        self.batch.clear();
                    }
                    return Some(0.0);
                }
                Err(TryRecvError::Disconnected) => return None,
            }
        }
        let Some(&sample) = self.chunk.get(self.index) else {
            return Some(0.0);
        };
        self.index += 1;
        if self.has_left {
            self.batch.push((self.left + sample) * 0.5);
            self.has_left = false;
            if self.batch.len() >= 512 {
                self.tap.push(&self.batch);
                self.batch.clear();
            }
        } else {
            self.left = sample;
            self.has_left = true;
        }
        Some(sample)
    }
}

impl Source for SpotifySource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(44_100).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub type Track = Tapped<Fx<Decoder<Input>>>;

pub struct Engine {
    device: MixerDeviceSink,
    player: Player,
    spotify_player: Player,
    spotify_tx: SyncSender<Vec<f32>>,
    pub tap: Arc<SampleTap>,
    pub eq: Arc<EqParams>,
    volume: f32,
    epoch: u64,
    fading: Vec<Fade>,
    fade_in: Option<(Instant, Duration)>,
}

struct Fade {
    player: Player,
    started: Instant,
    length: Duration,
}

impl Engine {
    pub fn new() -> Result<Self> {
        let mut device =
            DeviceSinkBuilder::open_default_sink().context("no audio output device available")?;
        device.log_on_drop(false);
        let tap = SampleTap::new();
        let (spotify_tx, spotify_rx) = sync_channel::<Vec<f32>>(24);
        let spotify_player = Player::connect_new(device.mixer());
        spotify_player.append(SpotifySource {
            rx: spotify_rx,
            chunk: Vec::new(),
            index: 0,
            tap: tap.clone(),
            left: 0.0,
            has_left: false,
            batch: Vec::with_capacity(512),
        });
        let player = Player::connect_new(device.mixer());
        let mut engine = Self {
            device,
            player,
            spotify_player,
            spotify_tx,
            tap,
            eq: EqParams::new(),
            volume: 0.7,
            epoch: 0,
            fading: Vec::new(),
            fade_in: None,
        };
        engine.apply_volume();
        Ok(engine)
    }

    pub fn spotify_sender(&self) -> SyncSender<Vec<f32>> {
        self.spotify_tx.clone()
    }

    pub fn tap(&self) -> Arc<SampleTap> {
        self.tap.clone()
    }

    fn apply_volume(&mut self) {
        if self.fade_in.is_none() {
            self.player.set_volume(self.volume);
        }
        self.spotify_player.set_volume(self.volume);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.apply_volume();
    }

    pub fn change_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        self.apply_volume();
    }

    pub fn open_file(
        path: &Path,
        tap: Arc<SampleTap>,
        eq: Arc<EqParams>,
        gain: f32,
    ) -> Result<Track> {
        let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let len = file.metadata()?.len();
        let mut builder = Decoder::builder()
            .with_data(Input::File(BufReader::new(file)))
            .with_byte_len(len)
            .with_seekable(true);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            builder = builder.with_hint(ext);
        }
        let decoder = builder
            .build()
            .map_err(|e| anyhow!("cannot decode {}: {e}", path.display()))?;
        Ok(Tapped::new(Fx::new(decoder, eq, gain), tap))
    }

    pub fn open_stream(
        stream: crate::radio::BufferedStream,
        mime: &str,
        tap: Arc<SampleTap>,
        eq: Arc<EqParams>,
    ) -> Result<Track> {
        let mut builder = Decoder::builder()
            .with_data(Input::Stream(Mutex::new(stream)))
            .with_seekable(false);
        if !mime.is_empty() {
            builder = builder.with_mime_type(mime);
        }
        let decoder = builder
            .build()
            .map_err(|e| anyhow!("cannot decode stream: {e}"))?;
        Ok(Tapped::new(Fx::new(decoder, eq, 1.0), tap))
    }

    pub fn play(&mut self, track: Track) -> Option<Duration> {
        let duration = track.total_duration();
        self.fading.clear();
        self.fade_in = None;
        self.epoch += 1;
        self.player = Player::connect_new(self.device.mixer());
        self.player.set_volume(self.volume);
        self.tap.clear();
        self.player.append(track);
        self.player.play();
        duration
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn queue_next(&mut self, track: Track) -> Option<Duration> {
        let duration = track.total_duration();
        self.player.append(track);
        duration
    }

    pub fn sources(&self) -> usize {
        self.player.len()
    }

    pub fn crossfade_to(&mut self, track: Track, length: Duration) -> Option<Duration> {
        let duration = track.total_duration();
        let old = std::mem::replace(&mut self.player, Player::connect_new(self.device.mixer()));
        self.fading.push(Fade {
            player: old,
            started: Instant::now(),
            length,
        });
        self.epoch += 1;
        self.player.set_volume(0.0);
        self.player.append(track);
        self.player.play();
        self.fade_in = Some((Instant::now(), length));
        duration
    }

    pub fn update(&mut self) {
        if let Some((started, length)) = self.fade_in {
            let t = (started.elapsed().as_secs_f32() / length.as_secs_f32().max(0.001)).min(1.0);
            self.player.set_volume(self.volume * t);
            if t >= 1.0 {
                self.fade_in = None;
            }
        }
        self.fading
            .retain(|fade| fade.started.elapsed() < fade.length);
        for fade in &self.fading {
            let t = fade.started.elapsed().as_secs_f32() / fade.length.as_secs_f32().max(0.001);
            fade.player
                .set_volume(self.volume * (1.0 - t).clamp(0.0, 1.0));
        }
    }

    pub fn stop(&mut self) {
        self.fading.clear();
        self.fade_in = None;
        self.epoch += 1;
        self.player = Player::connect_new(self.device.mixer());
        self.player.set_volume(self.volume);
        self.tap.clear();
    }

    pub fn toggle_pause(&self) -> bool {
        if self.player.is_paused() {
            self.player.play();
            self.fading.iter().for_each(|f| f.player.play());
            false
        } else {
            self.player.pause();
            self.fading.iter().for_each(|f| f.player.pause());
            true
        }
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub fn is_finished(&self) -> bool {
        self.player.empty()
    }

    pub fn position(&self) -> Duration {
        self.player.get_pos()
    }

    pub fn seek_to(&self, target: Duration) -> bool {
        self.player.try_seek(target).is_ok()
    }

    pub fn seek_by(&self, forward: bool, step: Duration, duration: Option<Duration>) -> bool {
        let current = self.player.get_pos();
        let mut target = if forward {
            current + step
        } else {
            current.saturating_sub(step)
        };
        if let Some(total) = duration {
            target = target.min(total.saturating_sub(Duration::from_millis(500)));
        }
        self.player.try_seek(target).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_local_wav() {
        let dir = std::env::temp_dir().join(format!("crab-engine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        crate::testutil::write_wav(&path, 3);
        let tap = SampleTap::new();
        let mut track = Engine::open_file(&path, tap.clone(), EqParams::new(), 1.0).unwrap();
        let count = (&mut track).take(44_100 * 2).count();
        assert_eq!(count, 44_100 * 2);
        assert!(track.total_duration().is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
