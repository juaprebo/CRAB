use anyhow::Result;
use std::path::Path;

pub trait AudioDecoder {
    type Source;
    fn open(&self, path: &Path, gain: f32) -> Result<Self::Source>;
}

pub fn replay_gain(db: Option<f32>, enabled: bool) -> f32 {
    if enabled {
        db.map_or(1.0, |db| 10f32.powf(db.clamp(-24.0, 12.0) / 20.0))
    } else {
        1.0
    }
}
