use super::cover_display::Display;
use crate::shell::{
    application::run_player_service::RunPlayerService,
    domain::player_runtime::PlayerRuntime,
    presentation::{controller::App, terminal},
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use std::time::Duration;

struct TerminalRuntime<'a> {
    terminal: &'a mut DefaultTerminal,
    app: &'a mut App,
    covers: Display,
}
impl PlayerRuntime for TerminalRuntime<'_> {
    fn should_quit(&self) -> bool {
        self.app.quit
    }
    fn frame(&mut self) -> Result<()> {
        let width = self.terminal.size()?.width;
        self.app
            .tick(terminal::bar_count(width, &self.app.config.visualizer));
        self.terminal
            .draw(|frame| terminal::draw(frame, self.app))?;
        let pixels = crossterm::terminal::window_size()
            .ok()
            .filter(|size| size.columns > 0 && size.rows > 0 && size.width > 0 && size.height > 0)
            .map(|size| (size.width / size.columns, size.height / size.rows))
            .unwrap_or((8, 16));
        self.covers.render(
            &mut std::io::stdout(),
            self.app.displayed_cover(),
            self.app.cover_target(),
            pixels,
        )?;
        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => self.app.on_key(key),
                Event::Mouse(mouse) => self.app.on_mouse(mouse),
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let covers = Display::new();
    app.native_covers = covers.enabled;
    let mut runtime = TerminalRuntime {
        terminal,
        app,
        covers,
    };
    let result = RunPlayerService.execute(&mut runtime);
    let _ = runtime.covers.clear(&mut std::io::stdout());
    result
}
