use super::{
    engine::{Engine, Track},
    equalizer::EqParams,
};
use crate::playback::domain::{audio_decoder::AudioDecoder, spectrum::SampleTap};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

pub struct RodioDecoder {
    tap: Arc<SampleTap>,
    equalizer: Arc<EqParams>,
}

impl RodioDecoder {
    pub fn new(tap: Arc<SampleTap>, equalizer: Arc<EqParams>) -> Self {
        Self { tap, equalizer }
    }
}

impl AudioDecoder for RodioDecoder {
    type Source = Track;
    fn open(&self, path: &Path, gain: f32) -> Result<Track> {
        Engine::open_file(path, self.tap.clone(), self.equalizer.clone(), gain)
    }
}
