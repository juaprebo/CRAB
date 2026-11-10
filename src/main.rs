mod analyzer;
mod app;
mod art;
mod config;
mod engine;
mod finder;
mod library;
mod picker;
mod radio;
#[cfg(feature = "spotify")]
mod spotify;
#[cfg(not(feature = "spotify"))]
#[path = "spotify_stub.rs"]
mod spotify;
mod sysvol;
#[cfg(test)]
mod testutil;
mod theme;
mod theme_editor;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use engine::Engine;
use ratatui::DefaultTerminal;
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str = "CRAB - terminal audio player

Usage:
  crab [PATH...]   Start the player, scanning the given folders or files (default: last used folders, or ~/Music)
  crab login       Sign in to Spotify (Premium account required)
  crab --help      Show this help

Radio stations are read from ~/.config/crab/stations.toml
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

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        let width = terminal.size()?.width;
        app.tick(ui::bar_count(width, &app.config.visualizer));
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(33))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("login") => {
            println!("Opening the Spotify login page in your browser...");
            spotify::login()?;
            println!("Signed in. Start CRAB and open the Spotify tab.");
            return Ok(());
        }
        _ => {}
    }

    let mut roots: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    if roots.is_empty() {
        roots = library::load_saved_roots();
    }
    if roots.is_empty()
        && let Some(music) = dirs::audio_dir()
    {
        roots.push(music);
    }

    redirect_stderr();
    let engine = Engine::new()?;
    let mut app = App::new(engine, roots);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    app.shutdown();
    result
}
