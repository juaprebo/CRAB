use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub const BANDS: usize = 10;
const FREQS: [f32; BANDS] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const Q: f32 = 1.2;

pub const PRESETS: [(&str, [f32; BANDS]); 7] = [
    ("flat", [0.0; BANDS]),
    ("bass", [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("treble", [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 5.0, 6.0]),
    (
        "vocal",
        [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0],
    ),
    (
        "rock",
        [5.0, 4.0, 2.0, -1.0, -2.0, -1.0, 2.0, 4.0, 5.0, 5.0],
    ),
    (
        "electronic",
        [5.0, 4.0, 1.0, 0.0, -2.0, 2.0, 1.0, 3.0, 5.0, 5.0],
    ),
    (
        "loudness",
        [6.0, 4.0, 0.0, 0.0, -1.0, 0.0, -1.0, 0.0, 4.0, 2.0],
    ),
];

pub fn preset(name: &str) -> Option<[f32; BANDS]> {
    PRESETS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, gains)| *gains)
}

pub fn next_preset(name: &str) -> &'static str {
    let index = PRESETS
        .iter()
        .position(|(n, _)| n.eq_ignore_ascii_case(name))
        .map_or(0, |i| i + 1);
    PRESETS[index % PRESETS.len()].0
}

pub struct EqParams {
    gains: [AtomicU32; BANDS],
    version: AtomicU32,
}

impl EqParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gains: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            version: AtomicU32::new(1),
        })
    }

    pub fn set(&self, gains: &[f32; BANDS]) {
        for (slot, gain) in self.gains.iter().zip(gains) {
            slot.store(gain.clamp(-12.0, 12.0).to_bits(), Ordering::Relaxed);
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    fn snapshot(&self) -> [f32; BANDS] {
        std::array::from_fn(|i| f32::from_bits(self.gains[i].load(Ordering::Relaxed)))
    }
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    fn peaking(freq: f32, gain_db: f32, rate: f32) -> Self {
        if gain_db.abs() < 0.05 || freq >= rate * 0.45 {
            return Self {
                b0: 1.0,
                ..Self::default()
            };
        }
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * std::f32::consts::PI * freq / rate;
        let alpha = w.sin() / (2.0 * Q);
        let cos = w.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    z1: f32,
    z2: f32,
}

pub struct Fx<S: Source> {
    inner: S,
    params: Arc<EqParams>,
    version: u32,
    gain: f32,
    channels: usize,
    rate: f32,
    coefs: [Biquad; BANDS],
    active: bool,
    states: Vec<[State; BANDS]>,
    channel: usize,
}

impl<S: Source> Fx<S> {
    pub fn new(inner: S, params: Arc<EqParams>, gain: f32) -> Self {
        let channels = inner.channels().get() as usize;
        let rate = inner.sample_rate().get() as f32;
        Self {
            inner,
            params,
            version: 0,
            gain,
            channels,
            rate,
            coefs: [Biquad::default(); BANDS],
            active: false,
            states: vec![[State::default(); BANDS]; channels],
            channel: 0,
        }
    }

    fn refresh(&mut self) {
        let version = self.params.version.load(Ordering::Acquire);
        if version == self.version {
            return;
        }
        self.version = version;
        let gains = self.params.snapshot();
        self.active = gains.iter().any(|g| g.abs() >= 0.05);
        for (i, coef) in self.coefs.iter_mut().enumerate() {
            *coef = Biquad::peaking(FREQS[i], gains[i], self.rate);
        }
    }
}

impl<S: Source> Iterator for Fx<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let mut x = self.inner.next()?;
        if self.channel == 0 {
            self.refresh();
        }
        if self.active {
            let states = &mut self.states[self.channel];
            for (coef, state) in self.coefs.iter().zip(states.iter_mut()) {
                let y = coef.b0 * x + state.z1;
                state.z1 = coef.b1 * x - coef.a1 * y + state.z2;
                state.z2 = coef.b2 * x - coef.a2 * y;
                x = y;
            }
        }
        self.channel = (self.channel + 1) % self.channels;
        Some((x * self.gain).clamp(-1.0, 1.0))
    }
}

impl<S: Source> Source for Fx<S> {
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
        self.states
            .iter_mut()
            .for_each(|s| *s = [State::default(); BANDS]);
        self.inner.try_seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::source::SineWave;

    fn rms(params: Arc<EqParams>, gain: f32) -> f32 {
        let source = SineWave::new(1000.0)
            .amplify(0.25)
            .take_duration(Duration::from_millis(400));
        let samples: Vec<f32> = Fx::new(source, params, gain).collect();
        let tail = &samples[samples.len() / 2..];
        (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
    }

    #[test]
    fn flat_is_transparent_and_boost_raises_level() {
        let params = EqParams::new();
        let flat = rms(params.clone(), 1.0);
        let mut gains = [0.0; BANDS];
        gains[5] = 9.0;
        params.set(&gains);
        let boosted = rms(params, 1.0);
        assert!(boosted > flat * 1.8, "{boosted} vs {flat}");
    }

    #[test]
    fn replaygain_scales() {
        let params = EqParams::new();
        let base = rms(params.clone(), 1.0);
        let quiet = rms(
            params,
            crate::playback::domain::audio_decoder::replay_gain(Some(-6.0), true),
        );
        assert!((quiet / base - 0.501).abs() < 0.02);
        assert_eq!(
            crate::playback::domain::audio_decoder::replay_gain(None, true),
            1.0
        );
    }

    #[test]
    fn presets_cycle() {
        assert_eq!(next_preset("flat"), "bass");
        assert_eq!(next_preset("loudness"), "flat");
        assert_eq!(next_preset("unknown"), "flat");
        assert!(preset("ROCK").is_some());
    }
}
