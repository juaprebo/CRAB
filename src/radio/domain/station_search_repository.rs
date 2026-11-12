use super::station::Station;
use anyhow::Result;

#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub station: Station,
    pub codec: String,
    pub bitrate: u32,
    pub country: String,
}

pub trait StationSearchRepository: Send + Sync {
    fn search(&self, name: &str, country: &str) -> Result<Vec<Found>>;
}
