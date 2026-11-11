mod analyzer;
mod app;
mod art;
mod clock;
mod commands;
mod config;
mod cover;
mod cover_display;
mod engine;
mod eq;
mod finder;
mod keys;
mod library;
mod library_browser;
mod lyrics;
mod media;
mod picker;
mod playlist;
mod radio;
mod scrobble;
#[cfg(feature = "spotify")]
mod spotify;
#[cfg(not(feature = "spotify"))]
#[path = "spotify_stub.rs"]
mod spotify;
mod state;
mod stats;
mod sysvol;
#[cfg(test)]
mod testutil;
mod theme;
mod theme_editor;
mod ui;
mod viz;

use anyhow::Result;
use app::App;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use engine::Engine;
use ratatui::DefaultTerminal;
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str = "CRAB - terminal audio player

Usage:
  crab [PATH...]   Start the player, scanning the given folders or files (default: last used folders, or ~/Music)
  crab login       Sign in to Spotify (Premium account required)
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

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut covers = cover_display::Display::new();
    app.native_covers = covers.enabled;
    let result = run_loop(terminal, app, &mut covers);
    let _ = covers.clear(&mut std::io::stdout());
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    covers: &mut cover_display::Display,
) -> Result<()> {
    while !app.quit {
        let width = terminal.size()?.width;
        app.tick(ui::bar_count(width, &app.config.visualizer));
        terminal.draw(|frame| ui::draw(frame, app))?;
        let pixels = crossterm::terminal::window_size()
            .ok()
            .filter(|size| size.columns > 0 && size.rows > 0 && size.width > 0 && size.height > 0)
            .map(|size| (size.width / size.columns, size.height / size.rows))
            .unwrap_or((8, 16));
        covers.render(
            &mut std::io::stdout(),
            app.displayed_cover(),
            app.cover_target(),
            pixels,
        )?;
        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                _ => {}
            }
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
        Some("--version") | Some("-V") => {
            println!("crab {}", env!("CARGO_PKG_VERSION"));
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
    let mouse = app.config.mouse;
    if mouse {
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
    let result = run(&mut terminal, &mut app);
    if mouse {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    app.shutdown();
    result
}
