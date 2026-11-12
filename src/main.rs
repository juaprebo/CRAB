mod library;
mod localization;
mod playback;
mod playlists;
mod radio;
mod settings;
mod shell;
#[cfg(test)]
mod testutil;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use library::infrastructure::scanner as library_store;
use localization::presentation::messages::text as tr;
use playback::infrastructure::engine::Engine;
use playback::infrastructure::spotify;
use shell::presentation::controller::App;
use std::path::PathBuf;

const HELP: &str = "CRAB - terminal audio player

Usage:
  crab [PATH...]   Start the player, scanning the given folders or files (default: last used folders, or ~/Music)
  crab login       Sign in to Spotify (Premium account required)
  crab ctl CMD     Send a command to the CRAB that is open (crab ctl status prints JSON)
  crab --formats   List available audio formats and backends
  crab --version   Show the version
  crab --help      Show this help

Press ? inside the player for the key list.
Configuration: ~/.config/crab/config.toml
Radio stations: ~/.config/crab/stations.toml
";

#[cfg(unix)]
fn redirect_stderr() {
    use std::os::fd::AsRawFd;
    let Some(dir) = dirs::cache_dir().map(|d| d.join("crab")) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("crab.log"))
    {
        unsafe {
            libc::dup2(file.as_raw_fd(), 2);
        }
    }
}

#[cfg(not(unix))]
fn redirect_stderr() {}

fn requested_track(roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .find(|path| path.is_file() && library_store::is_audio(path))
        .cloned()
}

fn forward_to_running(paths: &[PathBuf]) -> bool {
    if paths.is_empty()
        || !paths
            .iter()
            .all(|path| path.is_file() && library_store::is_audio(path))
        || !settings::infrastructure::config::single_instance_enabled()
    {
        return false;
    }
    let files: Vec<PathBuf> = paths
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect();
    shell::infrastructure::control::open_in_running(&files)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let language = settings::infrastructure::config::configured_language();
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print!("{}", tr(language, HELP));
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("crab {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--forward") => {
            let paths: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
            std::process::exit(if forward_to_running(&paths) { 0 } else { 1 });
        }
        Some("ctl") => std::process::exit(shell::infrastructure::control::run_client(&args[1..])),
        Some("--formats") => {
            shell::presentation::cli::print_formats(language);
            return Ok(());
        }
        Some("login") => {
            println!(
                "{}",
                tr(
                    language,
                    "Opening the Spotify login page in your browser..."
                )
            );
            spotify::login()?;
            println!(
                "{}",
                tr(language, "Signed in. Start CRAB and open the Spotify tab.")
            );
            return Ok(());
        }
        _ => {}
    }

    let mut roots: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    let autoplay = requested_track(&roots);
    if forward_to_running(&roots) {
        return Ok(());
    }
    if roots.is_empty() {
        roots = library_store::load_saved_roots();
    }
    if roots.is_empty()
        && let Some(music) = dirs::audio_dir()
    {
        roots.push(music);
    }

    redirect_stderr();
    let engine = Engine::new()?;
    let mut app = App::new(engine, roots);
    app.control = shell::infrastructure::control::start();
    if let Some(path) = autoplay {
        app.request_autoplay(path);
    }

    let mut terminal = ratatui::init();
    let mouse = app.config.mouse;
    if mouse {
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
    let result = shell::infrastructure::terminal_runtime::run(&mut terminal, &mut app);
    if mouse {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    app.shutdown();
    result
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    #[test]
    fn only_explicit_audio_files_request_playback_in_argument_order() {
        let dir = crate::testutil::temp_dir("cli-play");
        let first = dir.join("z-first.wav");
        let second = dir.join("a-second.wav");
        crate::testutil::write_wav(&first, 1);
        crate::testutil::write_wav(&second, 1);
        let other = dir.join("image.png");
        std::fs::write(&other, b"not audio").unwrap();
        assert_eq!(requested_track(&[]), None);
        assert_eq!(requested_track(std::slice::from_ref(&dir)), None);
        assert_eq!(requested_track(std::slice::from_ref(&other)), None);
        assert_eq!(
            requested_track(&[other, first.clone(), second]),
            Some(first)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
