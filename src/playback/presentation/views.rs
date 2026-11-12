use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::playback::infrastructure::{artwork::Cover, spotify};
use crate::settings::presentation::theme::Theme;
use crate::shell::presentation::controller::{Active, App, Repeat, SpotifyStatus};
use crate::shell::presentation::widgets::*;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, List, ListItem, Paragraph, Wrap},
};
use ratatui::{symbols, widgets::Padding};

pub(crate) fn draw_cover_panel(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    cover: Option<&Cover>,
    native: bool,
    theme: &Theme,
) -> (Rect, Option<Rect>) {
    if area.width < 80 {
        return (area, None);
    }
    let width = (area.width / 3).clamp(28, 40);
    let [list, art] =
        Layout::horizontal([Constraint::Min(35), Constraint::Length(width)]).areas(area);
    let block = panel(theme, tr(lang, "Album cover").into());
    let inner = block.inner(art);
    frame.render_widget(block, art);
    if let Some(cover) = cover {
        if !native {
            let cells = cover.cells(inner.width, inner.height);
            paint_art(frame.buffer_mut(), &cells, inner, theme, 1.0);
        }
    } else {
        frame.render_widget(
            Paragraph::new(tr(lang, "No cover art"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.dim)),
            inner,
        );
    }
    (list, Some(inner))
}

pub(crate) fn draw_queue_view(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    let Some(mut state) = app.queue_view.take() else {
        return;
    };
    let width = area.width.saturating_sub(4).min(64);
    let height = (app.up_next.len() as u16 + 4).clamp(6, area.height.saturating_sub(2).max(6));
    if width >= 30 && height <= area.height {
        let rect = centered(area, width, height);
        let inner = overlay_block(
            frame,
            rect,
            theme,
            tr_message(
                lang,
                "Up next - {arg0}",
                &[("arg0", (app.up_next.len()).to_string())],
            ),
        );
        let [list_area, help_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        if app.up_next.is_empty() {
            frame.render_widget(
                Paragraph::new(tr(lang, "Nothing queued. Press Q on a track to queue it"))
                    .style(Style::default().fg(theme.dim)),
                list_area,
            );
        } else {
            let items: Vec<ListItem> = app
                .up_next
                .iter()
                .map(|track| {
                    aligned_row(
                        list_area.width as usize,
                        "  ",
                        &track.title,
                        &track.detail(),
                        Style::default().fg(theme.text),
                        theme,
                    )
                })
                .collect();
            frame.render_stateful_widget(
                List::new(items).highlight_style(highlight(theme)),
                list_area,
                &mut state,
            );
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                tr(lang, "⏎ play now  x remove  X clear  esc close"),
                Style::default().fg(theme.dim),
            )),
            help_area,
        );
    }
    app.queue_view = Some(state);
}

pub(crate) fn draw_lyrics(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    let Some(offset) = app.lyrics_view else {
        return;
    };
    let width = area.width.saturating_sub(4).min(80);
    let height = area.height.saturating_sub(2);
    if width < 30 || height < 6 {
        return;
    }
    let Some(lyrics) = app.lyrics.clone() else {
        return;
    };
    let rect = centered(area, width, height);
    let title = tr_message(
        lang,
        "Lyrics · {arg0}",
        &[("arg0", (app.title).to_string())],
    );
    let inner = overlay_block(frame, rect, theme, title);
    let [list_area, help_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let visible = list_area.height as usize;
    let total = lyrics.lines.len();
    let current = lyrics.current(app.position());
    let start = match current {
        Some(index) => index
            .saturating_sub(visible / 2)
            .min(total.saturating_sub(visible)),
        None => {
            let start = offset.min(total.saturating_sub(visible));
            app.lyrics_view = Some(start);
            start
        }
    };
    let lines: Vec<Line> = lyrics
        .lines
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(index, (_, text))| {
            let style = if Some(index) == current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else if lyrics.synced() {
                Style::default().fg(theme.dim)
            } else {
                Style::default().fg(theme.text)
            };
            Line::styled(format!("  {text}"), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    let hint = if lyrics.synced() {
        tr(lang, "synced  esc close")
    } else {
        tr(lang, "up/down scroll  esc close")
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(theme.dim))),
        help_area,
    );
}

pub(crate) fn draw_now_playing(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    if area.width == 0 || area.height == 0 {
        app.hit.progress = Rect::default();
        return;
    }
    let compact = area.height < 4;
    let inner = if compact {
        area
    } else {
        let block = panel(theme, tr(lang, "Now playing").to_string());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    };
    let [top, bottom] = if inner.height == 1 {
        [inner, Rect::new(inner.x, inner.bottom(), inner.width, 0)]
    } else {
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner)
    };

    let (title, artist) = match app.active {
        Active::Spotify => (app.sp.title.clone(), app.sp.artist.clone()),
        Active::Radio(_) => {
            let live = app.live_title();
            if live.is_empty() {
                (app.title.clone(), app.artist.clone())
            } else {
                (live, app.title.clone())
            }
        }
        Active::Local(_) | Active::Queue(_) | Active::Extra => {
            (app.title.clone(), app.artist.clone())
        }
        Active::None => (String::from(tr(lang, "Nothing playing")), String::new()),
    };
    let icon = if app.loading {
        "…"
    } else if app.active == Active::None {
        "■"
    } else if app.is_playing() {
        "▶"
    } else {
        "⏸"
    };
    let mut spans = vec![
        Span::styled(format!("{icon} "), Style::default().fg(theme.accent)),
        Span::styled(
            title,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    let mut detail = artist;
    if matches!(
        app.active,
        Active::Local(_) | Active::Queue(_) | Active::Extra
    ) && !compact
        && !app.album.is_empty()
    {
        if detail.is_empty() {
            detail = app.album.clone();
        } else {
            detail = format!("{detail} - {}", app.album);
        }
    }
    if !detail.is_empty() {
        spans.push(Span::styled(
            format!("  -  {detail}"),
            Style::default().fg(theme.dim),
        ));
    }
    let mut badges: Vec<String> = Vec::new();
    if app.shuffle {
        badges.push(tr(lang, "shuffle").to_string());
    }
    if app.repeat != Repeat::Off {
        badges.push(tr_message(
            lang,
            "repeat {arg0}",
            &[("arg0", tr(lang, app.repeat.label()).to_string())],
        ));
    }
    if !app.up_next.is_empty() {
        badges.push(tr_message(
            lang,
            "{arg0} up next",
            &[("arg0", (app.up_next.len()).to_string())],
        ));
    }
    if app.config.audio.eq_gains().iter().any(|g| g.abs() >= 0.05) {
        badges.push(tr_message(
            lang,
            "eq {arg0}",
            &[("arg0", (app.config.audio.eq).to_string())],
        ));
    }
    if let Some(stats) = app.stats_badge() {
        badges.push(stats);
    }
    if app.visual_active() {
        badges.push(tr(lang, "visual").to_string());
    }
    if let Some(left) = app.sleep_left() {
        badges.push(tr_message(
            lang,
            "sleep {arg0}m",
            &[("arg0", (left.as_secs().div_ceil(60)).to_string())],
        ));
    }
    if compact {
        badges.clear();
    }
    let badge_text = badges.join(" · ");
    let badge_width = if badge_text.is_empty() {
        0
    } else {
        badge_text.chars().count() as u16 + 1
    };
    let [title_area, badge_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(badge_width)]).areas(top);
    frame.render_widget(Paragraph::new(Line::from(spans)), title_area);
    if badge_width > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                badge_text,
                Style::default().fg(theme.secondary),
            ))
            .alignment(Alignment::Right),
            badge_area,
        );
    }

    let volume_width = if !compact {
        22
    } else if inner.width >= 32 {
        9
    } else if inner.width >= 18 {
        5
    } else {
        0
    };
    let [progress, volume] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(volume_width)]).areas(bottom);
    app.hit.progress = progress;

    let position = app.position();
    let (ratio, label) = match app.total() {
        Some(total) if !total.is_zero() => (
            (position.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0),
            format!("{} / {}", format_time(position), format_time(total)),
        ),
        _ => match app.active {
            Active::Radio(_) if !app.loading => (
                0.0,
                tr_message(
                    lang,
                    "LIVE {arg0}",
                    &[("arg0", (format_time(position)).to_string())],
                ),
            ),
            _ => (0.0, String::from("--:--")),
        },
    };
    let label = if compact && progress.width < 18 {
        match app.active {
            Active::None => "--:--".to_string(),
            Active::Radio(_) => tr(lang, "LIVE").to_string(),
            _ => format_time(position),
        }
    } else {
        label
    };
    frame.render_widget(
        LineGauge::default()
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .unfilled_symbol(symbols::line::THICK_HORIZONTAL)
            .filled_style(Style::default().fg(theme.accent))
            .unfilled_style(Style::default().fg(theme.border))
            .label(Span::styled(label, Style::default().fg(theme.dim)))
            .ratio(ratio),
        progress,
    );

    if compact {
        let readout = if app.muted {
            tr(lang, "MUTE").to_string()
        } else if volume.width >= 9 {
            tr_message(
                lang,
                "VOL {arg0}%",
                &[("arg0", format!("{:.0}", app.volume() * 100.0))],
            )
        } else {
            format!("{:.0}%", app.volume() * 100.0)
        };
        frame.render_widget(
            Paragraph::new(readout)
                .style(Style::default().fg(theme.secondary))
                .alignment(Alignment::Right),
            volume,
        );
        return;
    }
    let level = if app.muted { 0.0 } else { app.volume() };
    let cells = 8usize;
    let filled = (level * cells as f32).round() as usize;
    let mut volume_spans = vec![Span::styled(
        tr(lang, "  VOL "),
        Style::default().fg(theme.dim),
    )];
    volume_spans.push(Span::styled(
        "█".repeat(filled),
        Style::default().fg(theme.secondary),
    ));
    volume_spans.push(Span::styled(
        "░".repeat(cells - filled),
        Style::default().fg(theme.border),
    ));
    let readout = if app.muted {
        tr(lang, " MUTE").to_string()
    } else {
        format!(" {:>3}%", (app.volume() * 100.0).round() as u32)
    };
    volume_spans.push(Span::styled(readout, Style::default().fg(theme.dim)));
    frame.render_widget(Paragraph::new(Line::from(volume_spans)), volume);
}

pub(crate) fn draw_spotify(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let lang = app.config.language;
    let mut lines: Vec<Line> = Vec::new();
    match &app.sp.status {
        SpotifyStatus::LoggedOut if !spotify::ENABLED => {
            lines.push(Line::styled(
                tr(lang, "Spotify support is not included in this build"),
                Style::default()
                    .fg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                tr(
                    lang,
                    "Rebuild with default features to enable it: cargo build --release",
                ),
                Style::default().fg(theme.dim),
            ));
        }
        SpotifyStatus::LoggedOut => {
            lines.push(Line::styled(
                tr(lang, "Not signed in to Spotify"),
                Style::default()
                    .fg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                tr(lang, "Close CRAB and run:  crab login"),
                Style::default().fg(theme.text),
            ));
            lines.push(Line::styled(
                tr(lang, "A Spotify Premium account is required."),
                Style::default().fg(theme.dim),
            ));
        }
        SpotifyStatus::Connecting => {
            lines.push(Line::styled(
                tr(lang, "Connecting to Spotify..."),
                Style::default().fg(theme.secondary),
            ));
        }
        SpotifyStatus::Failed(message) => {
            lines.push(Line::styled(
                tr(lang, "Spotify connection failed"),
                Style::default()
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                message.clone(),
                Style::default().fg(theme.dim),
            ));
        }
        SpotifyStatus::Ready => {
            lines.push(Line::from(vec![
                Span::styled("● ", Style::default().fg(theme.ok)),
                Span::styled(
                    tr(lang, "CRAB is available as a Spotify Connect device"),
                    Style::default().fg(theme.text),
                ),
            ]));
            lines.push(Line::styled(
                tr(lang, "Open Spotify on your phone or desktop and pick \"CRAB\" from the devices menu."),
                Style::default().fg(theme.dim),
            ));
            lines.push(Line::raw(""));
            if app.sp.title.is_empty() {
                lines.push(Line::styled(
                    tr(lang, "Nothing playing yet"),
                    Style::default().fg(theme.dim),
                ));
            } else {
                lines.push(Line::styled(
                    app.sp.title.clone(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ));
                lines.push(Line::styled(
                    app.sp.artist.clone(),
                    Style::default().fg(theme.text),
                ));
                lines.push(Line::styled(
                    app.sp.album.clone(),
                    Style::default().fg(theme.dim),
                ));
            }
        }
    }
    if spotify::ENABLED {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            tr(
                lang,
                "Unofficial integration, not affiliated with or endorsed by Spotify.",
            ),
            Style::default().fg(theme.dim),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(theme, tr(lang, "Spotify").to_string()).padding(Padding::new(2, 2, 1, 0))),
        area,
    );
}
