use crate::radio::domain::{
    station::Station,
    station_search_repository::{Found, StationSearchRepository},
};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

const HOSTS: [&str; 3] = ["de1", "at1", "nl1"];
const FETCH_LIMIT: usize = 150;
const SHOWN_LIMIT: usize = 80;
const SUPPORTED_CODECS: [&str; 4] = ["mp3", "aac", "ogg", "vorbis"];

#[derive(Deserialize)]
struct Raw {
    name: Option<String>,
    url_resolved: Option<String>,
    url: Option<String>,
    codec: Option<String>,
    bitrate: Option<u32>,
    countrycode: Option<String>,
    hls: Option<u8>,
    lastcheckok: Option<u8>,
}

pub fn parse_results(body: &str) -> Result<Vec<Found>> {
    let raw: Vec<Raw> =
        serde_json::from_str(body).context("unexpected response from radio-browser")?;
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for item in raw {
        if item.lastcheckok == Some(0) || item.hls.unwrap_or(0) != 0 {
            continue;
        }
        let name = item.name.unwrap_or_default().trim().to_string();
        let url = item
            .url_resolved
            .filter(|u| !u.trim().is_empty())
            .or(item.url)
            .unwrap_or_default()
            .trim()
            .to_string();
        let codec = item.codec.unwrap_or_default();
        let lowered = codec.to_lowercase();
        let supported = SUPPORTED_CODECS.iter().any(|c| lowered.contains(c));
        let web = url.starts_with("http://") || url.starts_with("https://");
        if name.is_empty() || !web || !supported || !seen.insert(url.clone()) {
            continue;
        }
        found.push(Found {
            station: Station { name, url },
            codec: codec.to_uppercase(),
            bitrate: item.bitrate.unwrap_or(0),
            country: item.countrycode.unwrap_or_default().to_uppercase(),
        });
        if found.len() >= SHOWN_LIMIT {
            break;
        }
    }
    Ok(found)
}

fn fetch(name: &str, country: &str) -> Result<Vec<Found>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("CRAB/0.1")
        .timeout(Duration::from_secs(12))
        .build()
        .context("cannot build http client")?;
    let mut params: Vec<(&str, String)> = vec![
        ("hidebroken", "true".to_string()),
        ("order", "votes".to_string()),
        ("reverse", "true".to_string()),
        ("limit", FETCH_LIMIT.to_string()),
    ];
    if !name.trim().is_empty() {
        params.push(("name", name.trim().to_string()));
    }
    if !country.is_empty() {
        params.push(("countrycode", country.to_string()));
    }
    let mut last = anyhow!("no radio-browser server answered");
    for host in HOSTS {
        let base = format!("https://{host}.api.radio-browser.info/json/stations/search");
        let url = reqwest::Url::parse_with_params(&base, &params)?;
        let reply = client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text());
        match reply {
            Ok(body) => return parse_results(&body),
            Err(error) if error.is_status() => last = anyhow!("radio-browser answered: {error}"),
            Err(_) => last = anyhow!("cannot reach radio-browser, check your connection"),
        }
    }
    Err(last)
}

pub struct RadioBrowserRepository;
impl StationSearchRepository for RadioBrowserRepository {
    fn search(&self, name: &str, country: &str) -> Result<Vec<Found>> {
        fetch(name, country)
    }
}
