use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::settings::presentation::theme::Theme;
use crate::shell::presentation::controller::{Active, App};
use crate::shell::presentation::widgets::*;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    widgets::{List, ListItem, Paragraph},
};

pub(crate) fn draw_playlists(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    let row_width = area.width.saturating_sub(2) as usize;
    let (title, items, message): (String, Vec<ListItem>, &str) = match app.open_playlist {
        None => {
            let items = app
                .playlists
                .iter()
                .map(|list| {
                    let playing = app.playing_playlist(&list.name);
                    let count = tr_message(
                        lang,
                        "{arg0} tracks",
                        &[("arg0", (list.tracks.len()).to_string())],
                    );
                    let name_style = if playing {
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    };
                    aligned_row(
                        row_width,
                        if playing { "♪ " } else { "≡ " },
                        &list.name,
                        &count,
                        name_style,
                        theme,
                    )
                })
                .collect();
            (
                tr_message(
                    lang,
                    "Playlists - {arg0}",
                    &[("arg0", (app.playlists.len()).to_string())],
                ),
                items,
                tr(
                    lang,
                    "No playlists yet. Press a to create one, or mark tracks in Library and press w",
                ),
            )
        }
        Some(index) => {
            let list = app.playlists.get(index);
            let name = list.map(|l| l.name.clone()).unwrap_or_default();
            let tracks = list.map(|l| l.tracks.as_slice()).unwrap_or(&[]);
            let items = tracks
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let track = app.meta(path).cloned().unwrap_or_else(|| {
                        crate::library::infrastructure::scanner::to_track(path.clone())
                    });
                    let playing = app.active == Active::Queue(i) && app.playing_playlist(&name);
                    let missing = !path.exists();
                    let marker = if playing {
                        "♪ "
                    } else if app.marked_tracks.contains(&i) {
                        "+ "
                    } else {
                        "  "
                    };
                    let title_style = if playing {
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else if missing {
                        Style::default().fg(theme.error)
                    } else {
                        Style::default().fg(theme.text)
                    };
                    aligned_row(
                        row_width,
                        marker,
                        &track.title,
                        &track.detail(),
                        title_style,
                        theme,
                    )
                })
                .collect();
            let marked = if app.marked_tracks.is_empty() {
                String::new()
            } else {
                tr_message(
                    lang,
                    " - {arg0} marked",
                    &[("arg0", (app.marked_tracks.len()).to_string())],
                )
            };
            (
                tr_message(
                    lang,
                    "Playlist - {name} - {arg0} tracks{marked}",
                    &[
                        ("name", (name).to_string()),
                        ("arg0", (tracks.len()).to_string()),
                        ("marked", (marked).to_string()),
                    ],
                ),
                items,
                tr(lang, "Empty playlist. Press a to add files or folders"),
            )
        }
    };
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(theme.dim))
                .alignment(Alignment::Center)
                .block(panel(theme, title)),
            area,
        );
        return;
    }
    let state = match app.open_playlist {
        None => &mut app.playlist_state,
        Some(_) => &mut app.playlist_track_state,
    };
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(theme, title))
            .highlight_style(highlight(theme)),
        area,
        state,
    );
}
