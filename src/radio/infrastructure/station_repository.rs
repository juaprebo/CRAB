use crate::radio::domain::station::Station;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize)]
struct StationFile {
    #[serde(default)]
    station: Vec<Station>,
}

fn default_stations() -> Vec<Station> {
    [
        (
            "SomaFM - Groove Salad",
            "https://ice1.somafm.com/groovesalad-128-mp3",
        ),
        (
            "SomaFM - Secret Agent",
            "https://ice1.somafm.com/secretagent-128-mp3",
        ),
        (
            "SomaFM - DEF CON Radio",
            "https://ice1.somafm.com/defcon-128-mp3",
        ),
        (
            "SomaFM - Drone Zone",
            "https://ice1.somafm.com/dronezone-128-mp3",
        ),
        (
            "SomaFM - Metal Detector",
            "https://ice1.somafm.com/metal-128-mp3",
        ),
        (
            "Radio Paradise - Main Mix",
            "https://stream.radioparadise.com/mp3-192",
        ),
        (
            "Radio Paradise - Mellow Mix",
            "https://stream.radioparadise.com/mellow-192",
        ),
        (
            "Radio Paradise - Rock Mix",
            "https://stream.radioparadise.com/rock-192",
        ),
    ]
    .into_iter()
    .map(|(name, url)| Station {
        name: name.to_string(),
        url: url.to_string(),
    })
    .collect()
}

fn stations_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("crab").join("stations.toml"))
}

pub fn load_stations() -> Result<Vec<Station>> {
    let Some(path) = stations_path() else {
        return Ok(default_stations());
    };
    load_stations_from(&path)
}

fn load_stations_from(path: &std::path::Path) -> Result<Vec<Station>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let file: StationFile = toml::from_str(&text).context("cannot parse stations")?;
            Ok(file.station)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let stations = default_stations();
            save_stations_to(path, &stations)?;
            Ok(stations)
        }
        Err(error) => Err(error).context("cannot read stations"),
    }
}

pub fn save_stations(stations: &[Station]) -> Result<()> {
    let path = stations_path().ok_or_else(|| anyhow!("no config directory available"))?;
    save_stations_to(&path, stations)
}

fn save_stations_to(path: &std::path::Path, stations: &[Station]) -> Result<()> {
    let file = StationFile {
        station: stations.to_vec(),
    };
    let text = toml::to_string_pretty(&file).context("cannot encode stations")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("cannot create config directory")?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).context("cannot write stations")?;
    std::fs::rename(&temp, path).context("cannot replace stations")?;
    Ok(())
}

pub struct TomlStationRepository;
impl crate::radio::domain::station_repository::StationRepository for TomlStationRepository {
    fn load(&self) -> Result<Vec<Station>> {
        load_stations()
    }
    fn save(&self, stations: &[Station]) -> Result<()> {
        save_stations(stations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_file_is_initialized_and_empty_list_is_preserved() {
        let dir = crate::testutil::temp_dir("radio");
        let path = dir.join("stations.toml");
        let stations = load_stations_from(&path).unwrap();
        assert_eq!(stations, default_stations());
        assert_eq!(load_stations_from(&path).unwrap(), stations);
        save_stations_to(&path, &[]).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(load_stations_from(&path).unwrap().is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_station_file_is_preserved() {
        let dir = crate::testutil::temp_dir("radio");
        let path = dir.join("stations.toml");
        for original in [
            "[[station]]\nname = \"My station\"\nurl = [invalid\n",
            "[[station]]\nname = 123\nurl = \"https://example.com\"\n",
        ] {
            std::fs::write(&path, original).unwrap();
            assert!(load_stations_from(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn read_error_does_not_replace_station_path() {
        let dir = crate::testutil::temp_dir("radio");
        let path = dir.join("stations.toml");
        std::fs::create_dir(&path).unwrap();
        assert!(load_stations_from(&path).is_err());
        assert!(path.is_dir());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
