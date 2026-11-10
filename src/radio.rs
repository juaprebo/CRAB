use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREFILL_BYTES: usize = 96 * 1024;
const PREFILL_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub url: String,
}

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

pub fn load_stations() -> Vec<Station> {
    let Some(dir) = dirs::config_dir().map(|d| d.join("crab")) else {
        return default_stations();
    };
    let path = dir.join("stations.toml");
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(file) = toml::from_str::<StationFile>(&text)
        && !file.station.is_empty()
    {
        return file.station;
    }
    let stations = default_stations();
    let file = StationFile {
        station: stations.clone(),
    };
    if let Ok(text) = toml::to_string_pretty(&file) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, text);
    }
    stations
}

pub struct BufferedStream {
    rx: Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    pos: usize,
    queued: Arc<AtomicUsize>,
}

impl Read for BufferedStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.chunk.len() {
            match self.rx.recv_timeout(Duration::from_secs(6)) {
                Ok(chunk) => {
                    self.queued.fetch_sub(chunk.len(), Ordering::Relaxed);
                    self.chunk = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let available = &self.chunk[self.pos..];
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.pos += n;
        Ok(n)
    }
}

pub struct Connection {
    pub stream: BufferedStream,
    pub mime: String,
}

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("CRAB/0.1")
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("cannot build http client")
}

fn resolve_playlist(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
    let is_playlist = [".pls", ".m3u", ".m3u8"]
        .iter()
        .any(|ext| path.ends_with(ext));
    if !is_playlist {
        return Ok(url.to_string());
    }
    let body = client.get(url).send()?.error_for_status()?.text()?;
    for line in body.lines() {
        let line = line.trim();
        let candidate = match line.split_once('=') {
            Some((key, value)) if key.to_lowercase().starts_with("file") => value.trim(),
            _ => line,
        };
        if candidate.starts_with("http://") || candidate.starts_with("https://") {
            return Ok(candidate.to_string());
        }
    }
    Err(anyhow!("playlist contains no stream url"))
}

fn parse_stream_title(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest.find("';").or_else(|| rest.rfind('\''))?;
    let title = rest[..end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

fn pump(
    mut response: reqwest::blocking::Response,
    metaint: usize,
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    title: Arc<Mutex<String>>,
    queued: Arc<AtomicUsize>,
    finished: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; 8192];
    let mut until_meta = metaint;
    loop {
        let want = if metaint > 0 {
            until_meta.min(buf.len())
        } else {
            buf.len()
        };
        let n = match response.read(&mut buf[..want]) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        queued.fetch_add(n, Ordering::Relaxed);
        if tx.send(buf[..n].to_vec()).is_err() {
            break;
        }
        if metaint > 0 {
            until_meta -= n;
            if until_meta == 0 {
                let mut len_byte = [0u8; 1];
                if response.read_exact(&mut len_byte).is_err() {
                    break;
                }
                let len = len_byte[0] as usize * 16;
                if len > 0 {
                    let mut block = vec![0u8; len];
                    if response.read_exact(&mut block).is_err() {
                        break;
                    }
                    if let Some(new_title) = parse_stream_title(&block) {
                        *title.lock().unwrap() = new_title;
                    }
                }
                until_meta = metaint;
            }
        }
    }
    finished.store(true, Ordering::Relaxed);
}

pub fn connect(url: &str, title: Arc<Mutex<String>>) -> Result<Connection> {
    let client = client()?;
    let url = resolve_playlist(&client, url)?;
    let response = client
        .get(&url)
        .header("Icy-MetaData", "1")
        .send()
        .with_context(|| format!("cannot connect to {url}"))?
        .error_for_status()?;
    let headers = response.headers();
    let metaint = headers
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mime = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .unwrap_or_default();

    let (tx, rx) = sync_channel::<Vec<u8>>(1024);
    let queued = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    {
        let queued = queued.clone();
        let finished = finished.clone();
        std::thread::spawn(move || pump(response, metaint, tx, title, queued, finished));
    }

    let started = Instant::now();
    while queued.load(Ordering::Relaxed) < PREFILL_BYTES
        && !finished.load(Ordering::Relaxed)
        && started.elapsed() < PREFILL_TIMEOUT
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    if queued.load(Ordering::Relaxed) == 0 {
        return Err(anyhow!("stream delivered no data"));
    }

    Ok(Connection {
        stream: BufferedStream {
            rx,
            chunk: Vec::new(),
            pos: 0,
            queued,
        },
        mime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::SampleTap;
    use crate::engine::Engine;

    #[test]
    #[ignore]
    fn live_stream() {
        let title = Arc::new(Mutex::new(String::new()));
        let connection = connect(
            &std::env::var("CRAB_TEST_STREAM")
                .unwrap_or("https://ice1.somafm.com/groovesalad-128-mp3".to_string()),
            title.clone(),
        )
        .unwrap();
        let mut track =
            Engine::open_stream(connection.stream, &connection.mime, SampleTap::new()).unwrap();
        let count = (&mut track).take(44_100 * 2).count();
        assert!(count > 0);
    }
}
