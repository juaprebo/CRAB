//! Optional streaming decoder. Native formats never require FFmpeg.
use anyhow::{Context, Result, bail};
use rodio::{ChannelCount, SampleRate, Source};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

pub fn probe(path: &Path) -> Option<serde_json::Value> {
    let path = path.canonicalize().ok()?;
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=duration,sample_rate,channels:format=duration:format_tags",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

pub fn duration(metadata: &serde_json::Value) -> Option<Duration> {
    let seconds = metadata
        .get("format")?
        .get("duration")?
        .as_str()?
        .parse::<f64>()
        .ok()?;
    Duration::try_from_secs_f64(seconds).ok()
}

pub struct Decoder {
    path: PathBuf,
    child: Child,
    output: BufReader<ChildStdout>,
    first: Option<f32>,
    duration: Option<Duration>,
    channels: ChannelCount,
    rate: SampleRate,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self> {
        if !available() {
            bail!("this codec needs FFmpeg; install ffmpeg to enable additional formats");
        }
        let path = path.canonicalize().context("cannot resolve audio file")?;
        let metadata = probe(&path);
        let duration = metadata.as_ref().and_then(duration);
        let stream = metadata
            .as_ref()
            .and_then(|value| value.get("streams"))
            .and_then(|value| value.get(0));
        let channels = stream
            .and_then(|stream| stream.get("channels"))
            .and_then(|value| value.as_u64())
            .and_then(|value| u16::try_from(value).ok())
            .and_then(ChannelCount::new)
            .unwrap_or(ChannelCount::new(2).unwrap());
        let rate = stream
            .and_then(|stream| stream.get("sample_rate"))
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(SampleRate::new)
            .unwrap_or(SampleRate::new(48_000).unwrap());
        Self::at(&path, Duration::ZERO, duration, channels, rate)
    }

    fn at(
        path: &Path,
        position: Duration,
        duration: Option<Duration>,
        channels: ChannelCount,
        rate: SampleRate,
    ) -> Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-ss"])
            .arg(format!("{:.9}", position.as_secs_f64()))
            .arg("-i")
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
            ])
            .arg("-ar")
            .arg(rate.get().to_string())
            .arg("-ac")
            .arg(channels.get().to_string())
            .arg("pipe:1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("cannot start FFmpeg")?;
        let output = BufReader::new(child.stdout.take().context("FFmpeg output unavailable")?);
        let mut source = Self {
            path: path.to_path_buf(),
            child,
            output,
            first: None,
            duration,
            channels,
            rate,
        };
        source.first = Some(
            source
                .read_sample()
                .context("FFmpeg could not decode an audio track")?,
        );
        Ok(source)
    }

    fn read_sample(&mut self) -> Option<f32> {
        let mut bytes = [0; 4];
        self.output.read_exact(&mut bytes).ok()?;
        Some(f32::from_le_bytes(bytes))
    }
}

impl Iterator for Decoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.first.take().or_else(|| self.read_sample())
    }
}

impl Source for Decoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        let next = Self::at(
            &self.path,
            position,
            self.duration,
            self.channels,
            self.rate,
        )
        .map_err(|error| {
            rodio::source::SeekError::Other(Arc::new(std::io::Error::other(error.to_string())))
        })?;
        *self = next;
        Ok(())
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_durations_are_rejected() {
        for value in ["NaN", "inf", "-1", "not a number"] {
            assert!(duration(&serde_json::json!({"format":{"duration":value}})).is_none());
        }
        assert_eq!(
            duration(&serde_json::json!({"format":{"duration":"1.25"}})),
            Some(Duration::from_millis(1250))
        );
    }

    #[test]
    #[ignore = "requires FFmpeg and its test encoders"]
    fn decodes_and_seeks_real_formats() {
        assert!(available(), "install FFmpeg to run extended-format tests");
        let dir = crate::testutil::temp_dir("formats");
        for (name, codec) in [
            ("tone.mp3", "libmp3lame"),
            ("tone.wav", "pcm_s16le"),
            ("tone.flac", "flac"),
            ("tone.ogg", "libvorbis"),
            ("tone.aac", "aac"),
            ("tone.m4a", "aac"),
            ("tone.aiff", "pcm_s16be"),
            ("tone.mp2", "mp2"),
            ("tone.opus", "libopus"),
            ("tone.wma", "wmav2"),
            ("lossless.m4a", "alac"),
            ("tone.caf", "pcm_s16le"),
            ("tone.mka", "flac"),
            ("tone.wv", "wavpack"),
            ("tone.tta", "tta"),
            ("tone.au", "pcm_s16be"),
            ("tone.ac3", "ac3"),
        ] {
            let path = dir.join(name);
            let output = Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=2",
                    "-c:a",
                    codec,
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fixture {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                crate::library::is_audio(&path),
                "library must discover {name}"
            );
            let mut track = crate::engine::Engine::open_file(
                &path,
                crate::analyzer::SampleTap::new(),
                crate::eq::EqParams::new(),
                1.0,
            )
            .unwrap_or_else(|error| panic!("decode {name}: {error:#}"));
            let samples: Vec<_> = track.by_ref().take(4096).collect();
            assert_eq!(samples.len(), 4096, "{name}");
            assert!(samples.iter().all(|sample| sample.is_finite()), "{name}");
            assert!(samples.iter().any(|sample| sample.abs() > 0.001), "{name}");
        }
        let alac = Decoder::open(&dir.join("lossless.m4a")).unwrap();
        assert_eq!(alac.sample_rate().get(), 44_100);
        assert_eq!(alac.channels().get(), 1);
        drop(alac);
        let mut source = Decoder::open(&dir.join("tone.opus")).unwrap();
        assert!(source.total_duration().is_some());
        assert!(source.next().is_some());
        source.try_seek(Duration::from_millis(750)).unwrap();
        assert!(source.next().is_some());
        drop(source);
        let invalid = dir.join("broken.opus");
        std::fs::write(&invalid, b"not audio").unwrap();
        assert!(Decoder::open(&invalid).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
