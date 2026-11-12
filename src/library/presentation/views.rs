use crate::library::presentation::{
    browser::{Browser, duration_label},
    folder_picker::Picker,
};
use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::settings::presentation::theme::Theme;
use crate::shell::presentation::controller::{Active, App};
use crate::shell::presentation::widgets::*;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, Paragraph, Wrap},
};

pub(crate) fn draw_picker(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    picker: &mut Picker,
    theme: &Theme,
) {
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(2).min(22);
    if width < 24 || height < 8 {
        return;
    }
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let mut block = panel(
        theme,
        if picker.files {
            tr(lang, "Add to playlist").to_string()
        } else {
            tr(lang, "Open folder").to_string()
        },
    );
    if let Some(background) = theme.background {
        block = block.style(Style::default().bg(background));
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [path_area, list_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);

    let path_line = match &picker.input {
        Some(input) => Line::from(vec![
            Span::styled(tr(lang, "Go to: "), Style::default().fg(theme.dim)),
            Span::styled(input.clone(), Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.accent)),
        ]),
        None => Line::from(vec![Span::styled(
            picker.dir.display().to_string(),
            Style::default()
                .fg(theme.secondary)
                .add_modifier(Modifier::BOLD),
        )]),
    };
    frame.render_widget(Paragraph::new(path_line), path_area);

    let items: Vec<ListItem> = picker
        .entries
        .iter()
        .map(|name| {
            let label = if name == ".." {
                "↰ ..".to_string()
            } else {
                let marked = picker.marked.contains(&picker.dir.join(name));
                let mark = if marked { "+" } else { " " };
                if picker.is_dir_entry(name) {
                    format!("{mark}▸ {name}")
                } else {
                    format!("{mark}♪ {name}")
                }
            };
            ListItem::new(Line::styled(label, Style::default().fg(theme.text)))
        })
        .collect();
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight(theme)),
        list_area,
        &mut picker.state,
    );

    let help = if let Some(error) = &picker.error {
        Line::styled(error.clone(), Style::default().fg(theme.error))
    } else if picker.files {
        Line::styled(
            tr(
                lang,
                "⏎ open folder or add file  space mark  a add marked  A add all here  ⌫ up  ~ home  / type path  esc done",
            ),
            Style::default().fg(theme.dim),
        )
    } else {
        Line::styled(
            tr(
                lang,
                "⏎ open  ⌫ up  space use this folder  a add  ~ home  / type path  . hidden  esc cancel",
            ),
            Style::default().fg(theme.dim),
        )
    };
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: true }), help_area);
}

pub(crate) fn draw_library(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    if !app.browser.is_songs() {
        let playing = app.current_track_path().cloned();
        let filter = if app.filtering {
            tr_message(
                lang,
                " - filter: {arg0}█",
                &[("arg0", (app.filter).to_string())],
            )
        } else if app.filter.is_empty() {
            String::new()
        } else {
            tr_message(
                lang,
                " - filter: {arg0}",
                &[("arg0", (app.filter).to_string())],
            )
        };
        let context = format!(
            "{}{}{}",
            filter,
            if app.only_favorites {
                tr(lang, " - favorites")
            } else {
                ""
            },
            if app.scanning {
                tr(lang, " - scanning")
            } else {
                ""
            }
        );
        app.hit.body = draw_library_browser(
            lang,
            frame,
            area,
            &mut app.browser,
            playing.as_ref(),
            &context,
            theme,
        );
        return;
    }
    let folders = app.roots_label();
    let marked = if app.marked.is_empty() {
        String::new()
    } else {
        tr_message(
            lang,
            " - {arg0} marked",
            &[("arg0", (app.marked.len()).to_string())],
        )
    };
    let favorites = if app.only_favorites {
        tr(lang, " - favorites")
    } else {
        ""
    };
    let filter = if app.filtering {
        tr_message(
            lang,
            " - filter: {arg0}█",
            &[("arg0", (app.filter).to_string())],
        )
    } else if app.filter.is_empty() {
        String::new()
    } else {
        tr_message(
            lang,
            " - filter: {arg0}",
            &[("arg0", (app.filter).to_string())],
        )
    };
    let title = if app.scanning {
        tr_message(
            lang,
            "Library - {folders} - scanning",
            &[("folders", (folders).to_string())],
        )
    } else {
        tr_message(
            lang,
            "Library - {folders} - {arg0} tracks{favorites}{marked}{filter}",
            &[
                ("folders", (folders).to_string()),
                ("arg0", (app.tracks.len()).to_string()),
                ("favorites", (favorites).to_string()),
                ("marked", (marked).to_string()),
                (tr(lang, "filter"), (filter).to_string()),
            ],
        )
    };
    let items: Vec<ListItem> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = app.active == Active::Local(i);
            let marker = if playing {
                "♪ "
            } else if app.marked.contains(&track.path) {
                "+ "
            } else if app.library.favorites().contains(&track.path) {
                "♥ "
            } else {
                "  "
            };
            let title_style = if playing {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            aligned_row(
                area.width.saturating_sub(2) as usize,
                marker,
                &track.title,
                &track.detail(),
                title_style,
                theme,
            )
        })
        .collect();

    if items.is_empty() {
        let message = if app.scanning {
            tr(lang, "Scanning for audio files...")
        } else {
            tr(lang, "No audio files found. Press o to choose a folder")
        };
        let message = if app.only_favorites && app.filter.is_empty() {
            tr(lang, "No favorites yet, press f on a track")
        } else if app.filter.is_empty() {
            message
        } else {
            tr(lang, "Nothing matches the filter")
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(theme.dim))
                .alignment(Alignment::Center)
                .block(panel(theme, title)),
            area,
        );
        return;
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(theme, title))
            .highlight_style(highlight(theme)),
        area,
        &mut app.library_state,
    );
}

pub(crate) fn draw_library_browser(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    browser: &mut Browser,
    playing: Option<&std::path::PathBuf>,
    context: &str,
    theme: &Theme,
) -> Rect {
    let list_area = area;
    let title = if browser.is_album() {
        let duration = duration_label(browser.rows.iter().flat_map(|row| row.tracks.iter()));
        tr_message(
            lang,
            "{arg0} - {arg1} track{arg2} · {arg3}{context}",
            &[
                ("arg0", browser.heading(lang)),
                ("arg1", (browser.rows.len()).to_string()),
                (
                    "arg2",
                    (if browser.rows.len() == 1 { "" } else { "s" }).to_string(),
                ),
                ("arg3", tr(lang, &duration).to_string()),
                ("context", (context).to_string()),
            ],
        )
    } else {
        tr_message(
            lang,
            "Library - {arg0} - {arg1} groups{context}",
            &[
                ("arg0", browser.heading(lang)),
                ("arg1", (browser.rows.len()).to_string()),
                ("context", (context).to_string()),
            ],
        )
    };
    if browser.rows.is_empty() {
        frame.render_widget(
            Paragraph::new(tr(
                lang,
                "No matching music. Clear the filter or press o to choose a folder.",
            ))
            .style(Style::default().fg(theme.dim))
            .wrap(Wrap { trim: true })
            .block(panel(theme, title)),
            list_area,
        );
    } else {
        let items: Vec<ListItem> = browser
            .rows
            .iter()
            .map(|row| {
                let active = row.is_song()
                    && row
                        .tracks
                        .first()
                        .is_some_and(|track| Some(&track.path) == playing);
                let marker = if active {
                    "♪ "
                } else if row.is_song() {
                    "  "
                } else {
                    "› "
                };
                aligned_row(
                    list_area.width.saturating_sub(2) as usize,
                    marker,
                    &row.display_title(lang),
                    &row.display_detail(lang),
                    Style::default().fg(if active { theme.accent } else { theme.text }),
                    theme,
                )
            })
            .collect();
        frame.render_stateful_widget(
            List::new(items)
                .block(panel(theme, title))
                .highlight_style(highlight(theme)),
            list_area,
            &mut browser.state,
        );
    }
    list_area
}
