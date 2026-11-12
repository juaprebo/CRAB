use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::radio::presentation::finder::{Finder, Focus};
use crate::settings::presentation::theme::Theme;
use crate::shell::presentation::controller::{Active, App};
use crate::shell::presentation::widgets::*;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Wrap},
};

pub(crate) fn draw_finder(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    finder: &mut Finder,
    theme: &Theme,
) {
    let width = area.width.saturating_sub(4).min(78);
    let height = area.height.saturating_sub(2).min(24);
    if width < 36 || height < 10 {
        return;
    }
    let rect = centered(area, width, height);
    let inner = overlay_block(frame, rect, theme, tr(lang, "Find stations").to_string());
    let [name_area, country_area, message_area, list_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(field_line(
            tr(lang, "Name"),
            &finder.name,
            finder.focus == Focus::Name,
            theme,
        )),
        name_area,
    );
    frame.render_widget(
        Paragraph::new(field_line(
            tr(lang, "Country"),
            &finder.country,
            finder.focus == Focus::Country,
            theme,
        )),
        country_area,
    );

    let message_color = if finder.message_is_error {
        theme.error
    } else {
        theme.secondary
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate(&finder.message, inner.width as usize),
            Style::default().fg(message_color),
        )),
        message_area,
    );

    let row_width = list_area.width as usize;
    let items: Vec<ListItem> = finder
        .results
        .iter()
        .map(|found| {
            let bitrate = if found.bitrate > 0 {
                format!(" {}k", found.bitrate)
            } else {
                String::new()
            };
            let info = format!("{} {}{}", found.country, found.codec, bitrate)
                .trim()
                .to_string();
            let name_width = row_width.saturating_sub(info.chars().count() + 3);
            let name = truncate(&found.station.name, name_width);
            let pad = row_width
                .saturating_sub(name.chars().count() + info.chars().count() + 2)
                .max(1);
            ListItem::new(Line::from(vec![
                Span::styled(name, Style::default().fg(theme.text)),
                Span::raw(" ".repeat(pad)),
                Span::styled(info, Style::default().fg(theme.dim)),
            ]))
        })
        .collect();
    let highlight_style = if finder.focus == Focus::Results {
        highlight(theme)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight_style),
        list_area,
        &mut finder.state,
    );

    let help = match finder.focus {
        Focus::Results => tr(
            lang,
            "↑↓ select  ⏎ save and play  / new search  tab fields  esc close",
        ),
        _ => tr(
            lang,
            "type to search  ⏎ search  ↓ results  tab next field  esc close",
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(help, Style::default().fg(theme.dim)))
            .wrap(Wrap { trim: true }),
        help_area,
    );
}

pub(crate) fn draw_radio(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    let items: Vec<ListItem> = app
        .stations
        .iter()
        .enumerate()
        .map(|(i, station)| {
            let playing = app.active == Active::Radio(i);
            let marker = if playing { "● " } else { "  " };
            let style = if playing {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.accent)),
                Span::styled(station.name.clone(), style),
            ]))
        })
        .collect();
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(
                theme,
                tr_message(
                    lang,
                    "Radio - {arg0} stations",
                    &[("arg0", (app.stations.len()).to_string())],
                ),
            ))
            .highlight_style(highlight(theme)),
        area,
        &mut app.radio_state,
    );
}
