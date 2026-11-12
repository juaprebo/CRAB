use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub const FFT_SIZE: usize = 2048;
const WAVE_LEN: usize = 1024;
const HISTORY_ROWS: usize = 64;
const HISTORY_EVERY: u32 = 3;

pub struct SampleTap {
    buffer: Mutex<VecDeque<f32>>,
    sample_rate: AtomicU32,
}

impl SampleTap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(VecDeque::with_capacity(FFT_SIZE * 2)),
            sample_rate: AtomicU32::new(44_100),
        })
    }

    pub fn push(&self, samples: &[f32]) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend(samples.iter().copied());
        while buffer.len() > FFT_SIZE * 2 {
            buffer.pop_front();
        }
    }

    pub fn set_sample_rate(&self, rate: u32) {
        self.sample_rate.store(rate.max(8_000), Ordering::Relaxed);
    }

    pub fn clear(&self) {
        self.buffer.lock().unwrap().clear();
    }

    fn snapshot(&self) -> (Vec<f32>, u32) {
        let buffer = self.buffer.lock().unwrap();
        let mut out = vec![0.0; FFT_SIZE];
        let take = buffer.len().min(FFT_SIZE);
        let start = buffer.len() - take;
        for (slot, sample) in out[FFT_SIZE - take..]
            .iter_mut()
            .zip(buffer.iter().skip(start))
        {
            *slot = *sample;
        }
        (out, self.sample_rate.load(Ordering::Relaxed))
    }
}

pub struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    levels: Vec<f32>,
    peaks: Vec<f32>,
    wave: Vec<f32>,
    history: VecDeque<Vec<f32>>,
    countdown: u32,
}

impl Spectrum {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| {
                let phase = std::f32::consts::TAU * i as f32 / (FFT_SIZE as f32 - 1.0);
                0.5 * (1.0 - phase.cos())
            })
            .collect();
        Self {
            fft,
            window,
            scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            levels: Vec::new(),
            peaks: Vec::new(),
            wave: Vec::new(),
            history: VecDeque::new(),
            countdown: HISTORY_EVERY,
        }
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    pub fn wave(&self) -> &[f32] {
        &self.wave
    }

    pub fn history(&self) -> &VecDeque<Vec<f32>> {
        &self.history
    }

    pub fn update(&mut self, bars: usize, tap: &SampleTap, active: bool) {
        let bars = bars.max(1);
        if self.levels.len() != bars {
            self.levels = vec![0.0; bars];
            self.peaks = vec![0.0; bars];
            self.history.clear();
        }

        let snapshot = if active { Some(tap.snapshot()) } else { None };
        let targets = match &snapshot {
            Some((samples, rate)) => self.analyze(bars, samples, *rate),
            None => vec![0.0; bars],
        };
        self.update_wave(snapshot.as_ref().map(|(samples, _)| samples.as_slice()));

        for ((level, peak), &target) in self
            .levels
            .iter_mut()
            .zip(self.peaks.iter_mut())
            .zip(targets.iter())
        {
            if target > *level {
                *level += (target - *level) * 0.65;
            } else {
                *level = (*level - 0.045).max(target).max(0.0);
            }
            if *level >= *peak {
                *peak = *level;
            } else {
                *peak = (*peak - 0.012).max(*level);
            }
        }

        self.countdown = self.countdown.saturating_sub(1);
        if self.countdown == 0 {
            self.countdown = HISTORY_EVERY;
            self.history.push_front(self.levels.clone());
            self.history.truncate(HISTORY_ROWS);
        }
    }

    fn update_wave(&mut self, samples: Option<&[f32]>) {
        match samples {
            Some(samples) => {
                let limit = FFT_SIZE - WAVE_LEN;
                let start = (1..limit)
                    .find(|&i| samples[i - 1] < 0.0 && samples[i] >= 0.0)
                    .unwrap_or(limit);
                self.wave.clear();
                self.wave
                    .extend_from_slice(&samples[start..start + WAVE_LEN]);
            }
            None => {
                for value in &mut self.wave {
                    *value *= 0.7;
                }
            }
        }
    }

    fn analyze(&mut self, bars: usize, samples: &[f32], rate: u32) -> Vec<f32> {
        for (i, slot) in self.scratch.iter_mut().enumerate() {
            *slot = Complex::new(samples[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        let bin_hz = rate as f32 / FFT_SIZE as f32;
        let nyquist = rate as f32 / 2.0;
        let low = 35.0_f32;
        let high = 16_000.0_f32.min(nyquist * 0.95);
        let ratio = (high / low).powf(1.0 / bars as f32);
        let max_bin = FFT_SIZE / 2 - 1;
        let norm = FFT_SIZE as f32 / 4.0;

        (0..bars)
            .map(|i| {
                let f_lo = low * ratio.powi(i as i32);
                let f_hi = f_lo * ratio;
                let center = (f_lo * f_hi).sqrt();
                let lo_bin = ((f_lo / bin_hz).floor() as usize).clamp(1, max_bin);
                let hi_bin = ((f_hi / bin_hz).ceil() as usize).clamp(lo_bin + 1, max_bin + 1);
                let mut peak = 0.0_f32;
                for bin in lo_bin..hi_bin {
                    peak = peak.max(self.scratch[bin].norm() / norm);
                }
                let db = 20.0 * peak.max(1e-7).log10() + 4.0 * (center / 1000.0).log2();
                ((db + 62.0) / 62.0).clamp(0.0, 1.0)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, count: usize) -> Vec<f32> {
        (0..count)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin() * 0.8)
            .collect()
    }

    #[test]
    fn sine_bar() {
        let tap = SampleTap::new();
        tap.set_sample_rate(44_100);
        tap.push(&sine(1000.0, 44_100, FFT_SIZE));
        let mut spectrum = Spectrum::new();
        let bars = 48;
        for _ in 0..8 {
            spectrum.update(bars, &tap, true);
        }
        let levels = spectrum.levels();
        let (loudest, _) = levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        let ratio = (16_000.0_f32 / 35.0).powf(1.0 / bars as f32);
        let expected = ((1000.0_f32 / 35.0).ln() / ratio.ln()) as usize;
        assert!(
            loudest.abs_diff(expected) <= 1,
            "loudest {loudest} expected {expected}"
        );
        assert!(levels[loudest] > 0.8);
        assert!(levels[0] < 0.4);
    }

    #[test]
    fn wave_triggers_on_rising_crossing() {
        let tap = SampleTap::new();
        tap.set_sample_rate(44_100);
        tap.push(&sine(440.0, 44_100, FFT_SIZE));
        let mut spectrum = Spectrum::new();
        spectrum.update(16, &tap, true);
        let wave = spectrum.wave();
        assert_eq!(wave.len(), WAVE_LEN);
        assert!(wave[0] >= 0.0 && wave[0] < 0.2);
        assert!(wave.iter().any(|v| *v > 0.7));
        for _ in 0..40 {
            spectrum.update(16, &tap, false);
        }
        assert!(spectrum.wave().iter().all(|v| v.abs() < 0.001));
    }

    #[test]
    fn history_scrolls_and_resets() {
        let tap = SampleTap::new();
        tap.push(&sine(1000.0, 44_100, FFT_SIZE));
        let mut spectrum = Spectrum::new();
        for _ in 0..(HISTORY_EVERY as usize * 5) {
            spectrum.update(24, &tap, true);
        }
        assert_eq!(spectrum.history().len(), 5);
        assert!(spectrum.history().iter().all(|row| row.len() == 24));
        for _ in 0..(HISTORY_EVERY as usize * HISTORY_ROWS) {
            spectrum.update(24, &tap, true);
        }
        assert_eq!(spectrum.history().len(), HISTORY_ROWS);
        spectrum.update(30, &tap, true);
        assert!(spectrum.history().len() <= 1);
    }

    #[test]
    fn silence() {
        let tap = SampleTap::new();
        tap.push(&sine(440.0, 44_100, FFT_SIZE));
        let mut spectrum = Spectrum::new();
        for _ in 0..5 {
            spectrum.update(32, &tap, true);
        }
        for _ in 0..60 {
            spectrum.update(32, &tap, false);
        }
        assert!(spectrum.levels().iter().all(|l| *l == 0.0));
    }
}
