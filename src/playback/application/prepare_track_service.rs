use crate::library::domain::library::LocalTrack;
use crate::playback::domain::audio_decoder::{AudioDecoder, replay_gain};
use anyhow::Result;

pub struct PrepareTrackService<D: AudioDecoder> {
    decoder: D,
}

impl<D: AudioDecoder> PrepareTrackService<D> {
    pub fn new(decoder: D) -> Self {
        Self { decoder }
    }
    pub fn execute(&self, track: &LocalTrack, replaygain: bool) -> Result<D::Source> {
        self.decoder
            .open(&track.path, replay_gain(track.gain_db, replaygain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    struct Decoder;
    impl AudioDecoder for Decoder {
        type Source = (PathBuf, f32);
        fn open(&self, path: &Path, gain: f32) -> Result<Self::Source> {
            Ok((path.into(), gain))
        }
    }
    #[test]
    fn prepares_the_selected_track_with_the_requested_replay_gain_policy() {
        let service = PrepareTrackService::new(Decoder);
        let mut track = crate::testutil::track("/music/example.mp3");
        track.gain_db = Some(-6.0);
        let (path, gain) = service.execute(&track, true).unwrap();
        assert_eq!(path, track.path);
        assert!((gain - 0.501).abs() < 0.001);
        assert_eq!(service.execute(&track, false).unwrap().1, 1.0);
    }
}
