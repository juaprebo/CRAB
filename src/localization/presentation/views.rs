use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::text as tr;
use crate::settings::presentation::theme::Theme;
use crate::shell::presentation::controller::App;
use crate::shell::presentation::widgets::*;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{List, ListItem, Paragraph, Wrap},
};

pub(crate) fn draw_language_picker(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(selected) = app.language_picker else {
        return;
    };
    let lang = app.config.language;
    let rect = centered(area, 54, 8);
    let inner = overlay_block(frame, rect, theme, tr(lang, "Language").to_string());
    let [heading, list_area, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(tr(lang, "Choose the interface language")),
        heading,
    );
    let items: Vec<ListItem> = Language::ALL
        .iter()
        .map(|language| ListItem::new(language.name()))
        .collect();
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight(theme)),
        list_area,
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(tr(lang, "↑↓ select  ⏎ apply  esc cancel")).wrap(Wrap { trim: true }),
        help,
    );
}
