use super::station::Station;
use anyhow::Result;

pub trait StationRepository {
    fn load(&self) -> Result<Vec<Station>>;
    fn save(&self, stations: &[Station]) -> Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum StationError {
    KeepOne,
    Missing,
}
impl std::fmt::Display for StationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::KeepOne => "Keep at least one station",
            Self::Missing => "Station no longer exists",
        })
    }
}
impl std::error::Error for StationError {}

pub fn validate_removal(stations: &[Station], index: usize) -> Result<(), StationError> {
    if index >= stations.len() {
        return Err(StationError::Missing);
    }
    if stations.len() <= 1 {
        return Err(StationError::KeepOne);
    }
    Ok(())
}
