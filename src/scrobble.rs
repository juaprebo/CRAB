use anyhow::{Result, anyhow};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://api.listenbrainz.org/1/submit-listens";

#[derive(Clone, Debug, PartialEq)]
pub struct Listen {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub started: u64,
    pub duration: Option<Duration>,
}

enum Job {
    Now(Listen),
    Done(Listen),
}

pub struct Scrobbler {
    tx: Sender<Job>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn payload(kind: &str, listen: &Listen) -> String {
    let mut info = serde_json::Map::new();
    info.insert("submission_client".into(), "crab".into());
    if let Some(duration) = listen.duration {
        info.insert("duration_ms".into(), (duration.as_millis() as u64).into());
    }
    let mut meta = serde_json::Map::new();
    meta.insert("artist_name".into(), listen.artist.clone().into());
    meta.insert("track_name".into(), listen.title.clone().into());
    if !listen.album.is_empty() {
        meta.insert("release_name".into(), listen.album.clone().into());
    }
    meta.insert("additional_info".into(), info.into());
    let mut entry = serde_json::Map::new();
    if kind == "single" {
        entry.insert("listened_at".into(), listen.started.into());
    }
    entry.insert("track_metadata".into(), meta.into());
    serde_json::json!({ "listen_type": kind, "payload": [entry] }).to_string()
}

fn submit(client: &reqwest::blocking::Client, token: &str, body: String) -> Result<()> {
    let response = client
        .post(ENDPOINT)
        .header("Authorization", format!("Token {token}"))
        .header("Content-Type", "application/json")
        .body(body)
        .send()?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("ListenBrainz answered {}", response.status()))
    }
}

pub fn eligible(duration: Option<Duration>, played: Duration) -> bool {
    match duration {
        Some(total) if total < Duration::from_secs(30) => false,
        Some(total) => played >= (total / 2).min(Duration::from_secs(240)),
        None => played >= Duration::from_secs(240),
    }
}

impl Scrobbler {
    pub fn start(token: &str) -> Option<Self> {
        let token = token.trim().to_string();
        if token.is_empty() {
            return None;
        }
        let (tx, rx) = channel::<Job>();
        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent(concat!("crab/", env!("CARGO_PKG_VERSION")))
                .build()
            else {
                return;
            };
            while let Ok(job) = rx.recv() {
                let (kind, listen) = match &job {
                    Job::Now(l) => ("playing_now", l),
                    Job::Done(l) => ("single", l),
                };
                let body = payload(kind, listen);
                let mut attempts = if kind == "single" { 3 } else { 1 };
                while attempts > 0 {
                    if submit(&client, &token, body.clone()).is_ok() {
                        break;
                    }
                    attempts -= 1;
                    if attempts > 0 {
                        std::thread::sleep(Duration::from_secs(5));
                    }
                }
            }
        });
        Some(Self { tx })
    }

    pub fn now_playing(&self, listen: Listen) {
        let _ = self.tx.send(Job::Now(listen));
    }

    pub fn listened(&self, listen: Listen) {
        let _ = self.tx.send(Job::Done(listen));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Listen {
        Listen {
            title: "Song".into(),
            artist: "Band".into(),
            album: "Album".into(),
            started: 1_700_000_000,
            duration: Some(Duration::from_secs(200)),
        }
    }

    #[test]
    fn payload_shape() {
        let single: serde_json::Value =
            serde_json::from_str(&payload("single", &sample())).unwrap();
        assert_eq!(single["listen_type"], "single");
        assert_eq!(single["payload"][0]["listened_at"], 1_700_000_000u64);
        assert_eq!(
            single["payload"][0]["track_metadata"]["artist_name"],
            "Band"
        );
        let now: serde_json::Value =
            serde_json::from_str(&payload("playing_now", &sample())).unwrap();
        assert!(now["payload"][0].get("listened_at").is_none());
    }

    #[test]
    fn eligibility_rules() {
        let total = Some(Duration::from_secs(200));
        assert!(!eligible(total, Duration::from_secs(99)));
        assert!(eligible(total, Duration::from_secs(100)));
        assert!(!eligible(
            Some(Duration::from_secs(20)),
            Duration::from_secs(20)
        ));
        assert!(eligible(
            Some(Duration::from_secs(900)),
            Duration::from_secs(240)
        ));
        assert!(!eligible(None, Duration::from_secs(100)));
    }

    #[test]
    fn empty_token_disables() {
        assert!(Scrobbler::start("  ").is_none());
    }
}
