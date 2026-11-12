use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub shuffle: bool,
    pub show_stats: bool,
    pub repeat: String,
    pub volume: Option<f32>,
    pub track: Option<PathBuf>,
    pub position_secs: u64,
    pub favorites: Vec<PathBuf>,
}
