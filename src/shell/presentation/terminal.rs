use crate::library::presentation::views::{draw_library, draw_picker};
use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message as tr_message, text as tr};
use crate::localization::presentation::views::draw_language_picker;
use crate::playback::domain::artwork::Cell;
use crate::playback::presentation::views::{
    draw_cover_panel, draw_lyrics, draw_now_playing, draw_queue_view, draw_spotify,
};
use crate::playlists::presentation::views::draw_playlists;
use crate::radio::presentation::views::{draw_finder, draw_radio};
use crate::settings::infrastructure::config::Config;
use crate::settings::infrastructure::config::{VisualizerConfig, VizStyle};
use crate::settings::presentation::theme::{Theme, from_hsl, to_hex};
use crate::settings::presentation::theme_editor::{ColorPicker, Editor, ThemePicker};
use crate::shell::infrastructure::clock;
use crate::shell::presentation::art::{self, Art};
use crate::shell::presentation::controller::{App, Tab};
use crate::shell::presentation::keys::Action;
use crate::shell::presentation::visualizers::{CurveView, MirrorView, WaterfallView, WaveView};
use crate::shell::presentation::widgets::{
    bar_line, centered, field_line, fit_row_text, highlight, overlay_block, paint_art, panel,
    swatch, swatch_color, truncate,
};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Tabs, Widget, Wrap};

const TABS_WIDTH: u16 = 50;
const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

pub fn bar_count(width: u16, config: &VisualizerConfig) -> usize {
    let inner = width.saturating_sub(2) as usize;
    let gap = config.gap() as usize;
    let step = config.bar_width() as usize + gap;
    ((inner + gap) / step).max(1)
}

struct SpectrumView<'a> {
    levels: &'a [f32],
    peaks: &'a [f32],
    theme: &'a Theme,
    bar_width: u16,
    gap: u16,
    show_peaks: bool,
}

impl Widget for SpectrumView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.levels.is_empty() {
            return;
        }
        let height = area.height as usize;
        let bars = self.levels.len();
        let step = self.bar_width + self.gap;
        let used = (bars as u16 * step).saturating_sub(self.gap);
        let offset = area.width.saturating_sub(used) / 2;

        for (i, (&level, &peak)) in self.levels.iter().zip(self.peaks.iter()).enumerate() {
            let x = area.x + offset + (i as u16) * step;
            if x + self.bar_width > area.x + area.width {
                break;
            }
            let eighths = (level.clamp(0.0, 1.0) * height as f32 * 8.0) as usize;
            let full = eighths / 8;
            let remainder = eighths % 8;
            let peak_row = ((peak.clamp(0.0, 1.0) * height as f32) as usize).min(height - 1);

            for row in 0..height {
                let y = area.y + (height - 1 - row) as u16;
                let t = (row as f32 + 0.5) / height as f32;
                let (symbol, color) = if row < full {
                    ('█', self.theme.gradient(t))
                } else if row == full && remainder > 0 {
                    (BLOCKS[remainder], self.theme.gradient(t))
                } else if self.show_peaks && row == peak_row && peak > 0.04 && row >= full {
                    ('▔', self.theme.gradient(1.0))
                } else {
                    continue;
                };
                for dx in 0..self.bar_width {
                    buf[(x + dx, y)].set_char(symbol).set_fg(color);
                }
            }
        }
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let lang = app.config.language;
    let theme = app.theme().clone();
    let area = frame.area();
    if let Some(background) = theme.background {
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            area,
        );
    }
    let screen = crate::shell::presentation::responsive::Screen::new(area);
    let (header, body, viz, now, footer) = (
        screen.header,
        screen.body,
        screen.viz,
        screen.now,
        screen.footer,
    );
    if screen.compact {
        app.hit.tabs = draw_compact_header(lang, frame, header, app.tab, &theme);
    } else {
        draw_header(frame, header, app, &theme);
        app.hit.tabs = locate_tabs(lang, frame.buffer_mut(), header);
    }
    app.cover_area = None;
    let body = if !screen.compact && matches!(app.tab, Tab::Library | Tab::Playlists | Tab::Spotify)
    {
        let (content, image) = draw_cover_panel(
            lang,
            frame,
            body,
            app.displayed_cover(),
            app.native_covers,
            &theme,
        );
        app.cover_area = image;
        content
    } else {
        body
    };
    app.hit.body = body;

    match app.tab {
        Tab::Library => draw_library(frame, body, app, &theme),
        Tab::Playlists => draw_playlists(frame, body, app, &theme),
        Tab::Radio => draw_radio(frame, body, app, &theme),
        Tab::Spotify => draw_spotify(frame, body, app, &theme),
    }
    if !screen.compact && (app.tab != Tab::Library || app.browser.is_songs()) {
        draw_art(frame, body, app, &theme);
    }
    if viz.height > 0 {
        draw_spectrum(frame, viz, app, &theme);
    }
    draw_now_playing(frame, now, app, &theme);
    draw_footer(frame, footer, app, &theme, screen.compact);
    if let Some(picker) = app.picker.as_mut() {
        draw_picker(lang, frame, area, picker, &theme);
    }
    draw_language_picker(frame, area, app, &theme);
    draw_name_prompt(frame, area, app, &theme);
    draw_queue_view(frame, area, app, &theme);
    draw_help(frame, area, app, &theme);
    draw_lyrics(frame, area, app, &theme);
    if let Some(finder) = app.finder.as_mut() {
        draw_finder(lang, frame, area, finder, &theme);
    }
    if let Some(picker) = app.theme_picker.as_mut() {
        draw_theme_picker(lang, frame, area, picker, &app.themes, &app.config, &theme);
    }
    if let Some(editor) = &app.theme_editor {
        draw_theme_editor(lang, frame, area, editor, &theme);
        if let Some(editing) = &editor.color {
            let title = tr_message(
                lang,
                "Color: {arg0}",
                &[("arg0", (editor.label(editor.selected)).to_string())],
            );
            let help = tr(
                lang,
                "←→ hue  ↑↓ lightness  PgUp/PgDn saturation  ⏎ ok  # hex  esc cancel",
            );
            let rows = editor.rows() as u16;
            let editor_top = area.y
                + area
                    .height
                    .saturating_sub((rows + 6).min(area.height.saturating_sub(2)))
                    / 2;
            let selected_row = editor_top + 2 + editor.selected as u16;
            draw_color_popup(
                lang,
                frame,
                area,
                &editing.picker,
                &title,
                None,
                help,
                Some(selected_row),
                &theme,
            );
        }
    }
    if let Some(generator) = &app.theme_generator {
        let mode = if generator.dark {
            tr(lang, "dark")
        } else {
            tr(lang, "light")
        };
        let help = tr(
            lang,
            "←→ hue  ↑↓ lightness  PgUp/PgDn saturation  m dark/light  ⏎ continue  esc cancel",
        );
        draw_color_popup(
            lang,
            frame,
            area,
            &generator.picker,
            tr(lang, "New theme: pick an accent color"),
            Some(mode),
            help,
            None,
            &theme,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_color_popup(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    picker: &ColorPicker,
    title: &str,
    mode: Option<&str>,
    help: &str,
    avoid_row: Option<u16>,
    theme: &Theme,
) {
    if area.width < 30 || area.height < 11 {
        return;
    }
    let height = if mode.is_some() { 10 } else { 9 };
    let width = 50.min(area.width);
    let height = height.min(area.height);
    let top = area.y + 1;
    let bottom = (area.y + area.height).saturating_sub(height + 1).max(top);
    let y = match avoid_row {
        Some(row) if row >= top && row < top + height => bottom,
        _ => top,
    };
    let rect = Rect::new(area.x + (area.width - width) / 2, y, width, height);
    let inner = overlay_block(frame, rect, theme, title.to_string());
    let mut constraints = vec![Constraint::Length(1); 3];
    constraints.push(Constraint::Length(1));
    if mode.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    let rows = Layout::vertical(constraints).split(inner);
    let bar_width = (inner.width as usize).saturating_sub(6).max(2);
    let (h, s, l) = (picker.h, picker.s, picker.l);

    frame.render_widget(
        Paragraph::new(bar_line(
            tr(lang, "Hue"),
            bar_width,
            h / 360.0,
            theme,
            |t| from_hsl(t * 360.0, s, l),
        )),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(bar_line(tr(lang, "Sat"), bar_width, s, theme, |t| {
            from_hsl(h, t, l)
        })),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(bar_line(tr(lang, "Light"), bar_width, l, theme, |t| {
            from_hsl(h, s, t)
        })),
        rows[2],
    );
    let rgb = picker.rgb();
    let mut preview = swatch_color(rgb, 6);
    preview.push(Span::styled(
        format!(
            "  {}  H {:.0}  S {:.0}%  L {:.0}%",
            to_hex(rgb),
            h,
            s * 100.0,
            l * 100.0
        ),
        Style::default().fg(theme.text),
    ));
    frame.render_widget(Paragraph::new(Line::from(preview)), rows[3]);
    let mut next = 4;
    if let Some(mode) = mode {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(tr(lang, "Mode  "), Style::default().fg(theme.dim)),
                Span::styled(mode.to_string(), Style::default().fg(theme.secondary)),
            ])),
            rows[next],
        );
        next += 1;
    }
    frame.render_widget(
        Paragraph::new(Line::styled(
            help.to_string(),
            Style::default().fg(theme.dim),
        ))
        .wrap(Wrap { trim: true }),
        rows[next],
    );
}

fn draw_theme_picker(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    picker: &mut ThemePicker,
    themes: &[Theme],
    config: &Config,
    theme: &Theme,
) {
    if area.width < 30 || area.height < 9 {
        return;
    }
    let height = (themes.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, 60, height);
    let inner = overlay_block(frame, rect, theme, tr(lang, "Theme").to_string());
    let [list_area, help_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let selected = picker.selected();
    let items: Vec<ListItem> = themes
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let custom = config
                .themes
                .keys()
                .any(|k| k.eq_ignore_ascii_case(&entry.name));
            let active = index == selected;
            let mut spans = swatch(entry, 6);
            spans.push(Span::raw("  "));
            let name_style = if active {
                Style::default()
                    .fg(theme.on_accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            spans.push(Span::styled(entry.name.clone(), name_style));
            if custom {
                let tag = if active { theme.on_accent } else { theme.dim };
                spans.push(Span::styled(tr(lang, "  custom"), Style::default().fg(tag)));
            }
            let item = ListItem::new(Line::from(spans));
            if active {
                item.style(Style::default().bg(theme.accent))
            } else {
                item
            }
        })
        .collect();
    frame.render_stateful_widget(List::new(items), list_area, &mut picker.state);
    let selected_name = themes
        .get(picker.selected())
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let help = if picker.confirm {
        Line::styled(
            tr_message(
                lang,
                "Delete \"{selected_name}\"? y to confirm, any other key cancels",
                &[("selected_name", (selected_name).to_string())],
            ),
            Style::default().fg(theme.error),
        )
    } else if let Some(notice) = &picker.notice {
        Line::styled(notice.clone(), Style::default().fg(theme.error))
    } else {
        Line::styled(
            tr(
                lang,
                "↑↓ preview  ⏎ use  e edit  n new  x delete  esc cancel",
            ),
            Style::default().fg(theme.dim),
        )
    };
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: true }), help_area);
}

fn draw_theme_editor(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    theme: &Theme,
) {
    if area.width < 34 || area.height < 11 {
        return;
    }
    let wanted = editor.rows() as u16 + 6;
    let rect = centered(area, 60, wanted.min(area.height.saturating_sub(2)));
    let inner = overlay_block(
        frame,
        rect,
        theme,
        tr_message(
            lang,
            "New theme: {arg0}",
            &[("arg0", (editor.name).to_string())],
        ),
    );
    let [preview, list_area, prompt, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(Line::from(swatch(&editor.draft, preview.width as usize))),
        preview,
    );

    let items: Vec<ListItem> = (0..editor.rows())
        .map(|row| {
            let mut spans = vec![Span::styled(
                format!("{:<11}", editor.label(row)),
                Style::default().fg(theme.text),
            )];
            match editor.get(row) {
                Some(rgb) => {
                    spans.push(Span::styled(
                        "███ ",
                        Style::default().fg(crate::settings::presentation::theme::to_color(rgb)),
                    ));
                    spans.push(Span::styled(to_hex(rgb), Style::default().fg(theme.dim)));
                }
                None => spans.push(Span::styled(
                    tr(lang, "none"),
                    Style::default().fg(theme.dim),
                )),
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(editor.selected));
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight(theme)),
        list_area,
        &mut state,
    );

    let prompt_line = if let Some(name) = &editor.naming {
        Line::from(vec![
            Span::styled(tr(lang, "Save as: "), Style::default().fg(theme.dim)),
            Span::styled(name.clone(), Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.accent)),
        ])
    } else if let Some(input) = &editor.input {
        Line::from(vec![
            Span::styled(tr(lang, "Hex: "), Style::default().fg(theme.dim)),
            Span::styled(input.clone(), Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.accent)),
        ])
    } else {
        Line::default()
    };
    let prompt_line = match &editor.error {
        Some(error) if editor.naming.is_none() && editor.input.is_none() => {
            Line::styled(error.clone(), Style::default().fg(theme.error))
        }
        Some(error) => {
            let mut spans = prompt_line.spans;
            spans.push(Span::styled(
                format!("  {error}"),
                Style::default().fg(theme.error),
            ));
            Line::from(spans)
        }
        None => prompt_line,
    };
    frame.render_widget(Paragraph::new(prompt_line), prompt);

    let help_text = if editor.naming.is_some() {
        tr(lang, "⏎ save to config.toml  esc back")
    } else if editor.input.is_some() {
        tr(lang, "type a hex color  ⏎ apply  esc cancel")
    } else {
        tr(
            lang,
            "⏎ color  # hex  d clear  +/- bars  s save  esc discard",
        )
    };
    frame.render_widget(
        Paragraph::new(Line::styled(help_text, Style::default().fg(theme.dim)))
            .wrap(Wrap { trim: true }),
        help,
    );
}

fn draw_art(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(art) = &app.art else {
        return;
    };
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let cells: Vec<Cell> = match art {
        Art::Crab => art::crab_cells(inner.width, inner.height),
        Art::Text(lines) => art::text_cells(lines, inner.width, inner.height),
    };
    paint_art(
        frame.buffer_mut(),
        &cells,
        inner,
        theme,
        app.config.art_strength(),
    );
}

fn locate_tabs(lang: Language, buf: &Buffer, header: Rect) -> Vec<(Rect, Tab)> {
    let row: Vec<&str> = (header.x..header.x + header.width)
        .map(|x| buf[(x, header.y)].symbol())
        .collect();
    let mut found = Vec::new();
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let label = format!("{} {}", i + 1, tr(lang, tab.label()));
        let chars: Vec<String> = label.chars().map(|c| c.to_string()).collect();
        let start = (0..row.len().saturating_sub(chars.len() - 1))
            .find(|&at| chars.iter().enumerate().all(|(k, c)| row[at + k] == c));
        if let Some(at) = start {
            let x = header.x + at as u16;
            found.push((
                Rect::new(
                    x.saturating_sub(1),
                    header.y,
                    label.chars().count() as u16 + 2,
                    1,
                ),
                *tab,
            ));
        }
    }
    found
}

fn draw_compact_header(
    lang: Language,
    frame: &mut Frame,
    area: Rect,
    current: Tab,
    theme: &Theme,
) -> Vec<(Rect, Tab)> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    if area.width < 30 {
        let text = fit_row_text(
            &format!("{} {}", current.index() + 1, tr(lang, current.label())),
            area.width as usize,
        );
        let width = Line::raw(text.as_str()).width() as u16;
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(theme.accent)),
            area,
        );
        return vec![(Rect::new(area.x, area.y, width, 1), current)];
    }
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut cursor = 0;
    if area.width >= 40 {
        spans.push(Span::styled(
            "CRAB  ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        cursor = 6;
    }
    for (label, tab) in [
        (tr(lang, "1 Lib"), Tab::Library),
        (tr(lang, "2 Lists"), Tab::Playlists),
        (tr(lang, "3 Radio"), Tab::Radio),
        (tr(lang, "4 Spot"), Tab::Spotify),
    ] {
        if cursor >= area.width {
            break;
        }
        let label = fit_row_text(label, (area.width - cursor) as usize);
        let width = Line::raw(label.as_str()).width() as u16;
        hits.push((Rect::new(area.x + cursor, area.y, width, 1), tab));
        let style = Style::default().fg(if current == tab {
            theme.accent
        } else {
            theme.dim
        });
        spans.push(Span::styled(label, style));
        cursor += width;
        if cursor < area.width {
            spans.push(Span::raw(" "));
            cursor += 1;
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    hits
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let lang = app.config.language;
    let wide = area.width >= 72;
    let logo_width = if wide { 26 } else { 10 };
    let theme_width = theme.name.chars().count() as u16 + 4;
    let clock_text = if app.config.clock.show {
        let text = clock::format_clock(
            clock::local_time(),
            app.config.clock.hour12,
            app.config.clock.seconds,
        );
        let width = text.chars().count() as u16 + 2;
        let room = area.width.saturating_sub(logo_width + theme_width);
        (room >= TABS_WIDTH + width).then_some((text, width))
    } else {
        None
    };
    let clock_width = clock_text.as_ref().map(|(_, width)| *width).unwrap_or(0);
    let [logo, tabs, clock_area, theme_name] = Layout::horizontal([
        Constraint::Length(logo_width),
        Constraint::Min(10),
        Constraint::Length(clock_width),
        Constraint::Length(theme_width),
    ])
    .areas(area);
    if let Some((text, _)) = clock_text {
        frame.render_widget(
            Paragraph::new(Span::styled(
                text,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            clock_area,
        );
    }
    let mut logo_spans = vec![
        Span::styled(" ▟█▙ ", Style::default().fg(theme.accent)),
        Span::styled(
            "CRAB",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ];
    if wide {
        logo_spans.push(Span::styled(
            "  RUST AUDIO BOX",
            Style::default().fg(theme.secondary),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(logo_spans)), logo);
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, tab)| Line::from(format!(" {} {} ", i + 1, tr(lang, tab.label()))))
        .collect();
    frame.render_widget(
        Tabs::new(titles)
            .select(app.tab.index())
            .style(Style::default().fg(theme.dim))
            .highlight_style(
                Style::default()
                    .fg(theme.on_accent)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" "),
        tabs,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{} ", theme.name),
            Style::default().fg(theme.dim),
        ))
        .alignment(Alignment::Right),
        theme_name,
    );
}

const HELP: &[(&str, &str)] = &[
    ("", "Playback"),
    ("@pause", "pause or resume"),
    ("@next / @prev", "next / previous"),
    ("@seek-back / @seek-forward", "seek 5 seconds (local files)"),
    ("@volume-up / @volume-down", "volume"),
    ("@mute", "mute"),
    ("@shuffle", "shuffle"),
    ("@repeat", "repeat off, all or one"),
    ("@queue", "queue the selected or marked tracks to play next"),
    ("@queue-view", "show the queue"),
    ("@sleep", "sleep timer: 15, 30, 60 and 90 minutes"),
    ("@eq", "equalizer preset"),
    ("@lyrics", "lyrics of the current track"),
    ("", "Navigation"),
    (
        "@tab-1 @tab-2 @tab-3 @tab-4",
        "Library, Playlists, Radio, Spotify",
    ),
    ("@tab-next", "next tab"),
    ("@up / @down", "select"),
    ("@play", "play"),
    ("gg / @bottom", "first / last (7G or 7gg jumps to row 7)"),
    ("@half-up / @half-down", "half page up / down"),
    (
        "5j 15k 9n",
        "a count (starting with 5-9) repeats the next action",
    ),
    ("@cancel", "cancel, clear the filter or go back"),
    ("", "Library"),
    ("@library-view", "switch songs, artists and albums"),
    ("@play", "open artist / album, or play album tracks"),
    (
        "@play-group",
        "play the selected group or the selected song's album",
    ),
    ("@queue", "queue the selected artist, album or track"),
    ("@back", "back to artists or albums"),
    ("@search", "filter by title, artist or album"),
    ("@sort", "sort by path, title, artist or album"),
    ("@mark / @mark-all", "mark a track / mark all"),
    ("@visual", "visual selection: move to mark a range"),
    ("@save", "save the marked tracks to a playlist"),
    ("@folder", "choose a music folder"),
    ("", "Playlists"),
    ("@mark", "new playlist / add files to an open playlist"),
    ("@open-list", "open the selected playlist"),
    (
        "@rename",
        "rename a Library file or playlist (Ctrl-u clears the name)",
    ),
    (
        "@track-mark / @track-mark-all",
        "mark a track / mark all (open playlist)",
    ),
    (
        "@remove",
        "delete the playlist, or remove the marked tracks",
    ),
    ("@import / @export", "import / export an .m3u file"),
    ("", "Radio"),
    ("@search", "find stations"),
    ("@remove", "remove the selected station"),
    ("", "Other"),
    (
        "@command",
        "command line (tab completes, up/down for history)",
    ),
    ("@favorite", "add or remove favorites"),
    ("@favorites", "show only favorites"),
    ("@stats", "show or hide CPU and memory used by CRAB"),
    ("@visualizer", "next visualizer"),
    ("@clock", "show or hide the clock"),
    ("@theme", "theme picker"),
    ("@reload", "reload the configuration"),
    ("@help", "this help"),
    ("@quit", "quit"),
    ("", "Commands"),
    (":language [en|es]", "change the interface language"),
    (
        ":sort :sleep :eq :repeat",
        "set a value directly, for example :sleep 30",
    ),
    (
        ":volume :seek :theme :viz",
        ":volume 60, :seek 1:30, :theme synthwave",
    ),
    (
        ":shuffle :mute :fade",
        ":fade 3 sets the crossfade in seconds",
    ),
    (
        ":play :pause :next :prev :stop",
        "playback; :play <text> plays a match",
    ),
    (":goto <text>", "select the first track that matches"),
    (":view songs|artists|albums", "choose the library view"),
    (":find <text>", "filter the library, :find alone clears it"),
    (":add <text>", "queue every track that matches"),
    (":fav :favs", "toggle a favorite, show only favorites"),
    (":queue :lyrics :clock :stats", "same as their keys"),
    (":tab :save :import :export", "jump and open dialogs"),
    (":folder :reload :help :quit", "same as their keys"),
    (":keys crab|vim|custom", "switch the key preset"),
    (
        ":map <action> <key>...",
        "rebind an action and save it to [keys]",
    ),
    (":unmap <action>", "remove the keys of an action"),
    (":reset <action>", "restore the original keys of an action"),
    (":map <action>", "show the keys of an action"),
];

fn help_keys(app: &App, spec: &str) -> String {
    let tokens: Vec<&str> = spec.split(' ').collect();
    if tokens.len() == 1
        && let Some(name) = tokens[0].strip_prefix('@')
        && let Some(action) = crate::shell::presentation::keys::action_by_name(name)
    {
        return app.keys.all(action);
    }
    tokens
        .iter()
        .map(|token| match token.strip_prefix('@') {
            Some(name) => crate::shell::presentation::keys::action_by_name(name)
                .map_or_else(|| (*token).to_string(), |a| app.keys.first(a)),
            None => (*token).to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_help(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let lang = app.config.language;
    let Some(offset) = app.help else {
        return;
    };
    let width = area.width.saturating_sub(4).min(90);
    let height = area.height.saturating_sub(2).min(HELP.len() as u16 + 3);
    if width < 40 || height < 8 {
        return;
    }
    let rect = centered(area, width, height);
    let inner = overlay_block(frame, rect, theme, tr(lang, "Keys").to_string());
    let [list_area, help_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let visible = list_area.height as usize;
    let offset = offset.min(HELP.len().saturating_sub(visible));
    app.help = Some(offset);
    let lines: Vec<Line> = HELP
        .iter()
        .filter(|(spec, _)| app.keys.vim() || !(spec.starts_with("gg") || spec.starts_with("5j")))
        .skip(offset)
        .take(visible)
        .map(|(spec, text)| {
            let key = help_keys(app, spec);
            if key.is_empty() {
                Line::styled(
                    crate::localization::presentation::messages::text(lang, text).to_string(),
                    Style::default()
                        .fg(theme.secondary)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("  {key:<28}"),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        crate::localization::presentation::messages::text(lang, text).to_string(),
                        Style::default().fg(theme.text),
                    ),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            tr(lang, "up/down scroll  esc close"),
            Style::default().fg(theme.dim),
        )),
        help_area,
    );
}

fn draw_name_prompt(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let lang = app.config.language;
    let Some(prompt) = &app.name_prompt else {
        return;
    };
    let width = area.width.saturating_sub(4).min(56);
    let height = (app.playlists.len() as u16 + 6)
        .clamp(6, 14)
        .min(area.height);
    if width < 24 || height < 5 {
        return;
    }
    let rect = centered(area, width, height);
    let inner = overlay_block(frame, rect, theme, tr(lang, prompt.title).to_string());
    let [field_area, list_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(field_line(
            tr(lang, prompt.label),
            &prompt.input,
            true,
            theme,
        )),
        field_area,
    );
    let current = crate::playlists::domain::playlist::find(&app.playlists, &prompt.input);
    let names_only = matches!(
        prompt.kind,
        crate::shell::presentation::controller::PromptKind::SaveMarked
            | crate::shell::presentation::controller::PromptKind::NewList
    );
    let lines: Vec<Line> = app
        .playlists
        .iter()
        .filter(|_| names_only)
        .enumerate()
        .map(|(i, list)| {
            let style = if Some(i) == current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.dim)
            };
            Line::styled(
                format!(
                    "  {} ({})",
                    truncate(&list.name, inner.width.saturating_sub(12) as usize),
                    list.tracks.len()
                ),
                style,
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    let help = if !names_only || app.playlists.is_empty() {
        tr(lang, "⏎ ok  esc cancel")
    } else {
        tr(lang, "⏎ ok  ↑↓ existing list  esc cancel")
    };
    frame.render_widget(
        Paragraph::new(Span::styled(help, Style::default().fg(theme.dim))),
        help_area,
    );
}

fn draw_spectrum(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let config = &app.config.visualizer;
    let block = panel(theme, config.style.title().to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let spectrum = &app.spectrum;
    match config.style {
        VizStyle::Bars => frame.render_widget(
            SpectrumView {
                levels: spectrum.levels(),
                peaks: spectrum.peaks(),
                theme,
                bar_width: config.bar_width(),
                gap: config.gap(),
                show_peaks: config.peaks,
            },
            inner,
        ),
        VizStyle::Mirror => frame.render_widget(
            MirrorView {
                levels: spectrum.levels(),
                theme,
                bar_width: config.bar_width(),
                gap: config.gap(),
            },
            inner,
        ),
        VizStyle::Line | VizStyle::Dots => frame.render_widget(
            CurveView {
                levels: spectrum.levels(),
                theme,
                stipple: config.style == VizStyle::Dots,
            },
            inner,
        ),
        VizStyle::Wave => frame.render_widget(
            WaveView {
                wave: spectrum.wave(),
                theme,
            },
            inner,
        ),
        VizStyle::Waterfall => frame.render_widget(
            WaterfallView {
                history: spectrum.history(),
                theme,
            },
            inner,
        ),
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, compact: bool) {
    let lang = app.config.language;
    if let Some(input) = &app.cmdline {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " :",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(input.clone(), Style::default().fg(theme.text)),
                Span::styled("▏", Style::default().fg(theme.accent)),
            ])),
            area,
        );
        let (hints, selected) = app.cmd_hints();
        let used = input.chars().count() as u16 + 4;
        if !hints.is_empty() && area.width > used + 4 {
            let mut spans = Vec::new();
            let mut width = 0u16;
            let room = area.width - used;
            for (i, hint) in hints.iter().enumerate() {
                let w = hint.chars().count() as u16 + 2;
                if width + w > room {
                    spans.push(Span::styled("…", Style::default().fg(theme.dim)));
                    break;
                }
                width += w;
                let style = if Some(i) == selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.dim)
                };
                spans.push(Span::styled(format!("{hint}  "), style));
            }
            let rect = Rect::new(area.x + used, area.y, area.width - used, 1);
            frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        }
        return;
    }
    if !app.status.is_empty() {
        let color = if app.status_is_error {
            theme.error
        } else {
            theme.secondary
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", app.status),
                Style::default().fg(color),
            )),
            area,
        );
        return;
    }
    let k = |action: Action| app.keys.first(action);
    let mut keys: Vec<(String, &str)> = vec![
        (
            format!("{}{}", k(Action::Up), k(Action::Down)),
            tr(lang, "select"),
        ),
        (k(Action::Play), tr(lang, "play")),
        (k(Action::Pause), tr(lang, "pause")),
        (
            format!("{}/{}", k(Action::Next), k(Action::Prev)),
            tr(lang, "next/prev"),
        ),
        (
            format!("{}{}", k(Action::SeekBack), k(Action::SeekForward)),
            tr(lang, "seek"),
        ),
        (
            format!("{}/{}", k(Action::VolumeUp), k(Action::VolumeDown)),
            tr(lang, "volume"),
        ),
        (k(Action::Mute), tr(lang, "mute")),
        (
            k(Action::Shuffle),
            if app.shuffle {
                tr(lang, "shuffle on")
            } else {
                tr(lang, "shuffle")
            },
        ),
        (k(Action::Repeat), tr(lang, "repeat")),
        (k(Action::Queue), tr(lang, "queue")),
        (k(Action::Visualizer), tr(lang, "viz")),
        (k(Action::Clock), tr(lang, "clock")),
        (k(Action::Folder), tr(lang, "folder")),
    ];
    if app.tab == Tab::Radio {
        keys.insert(2, (k(Action::Search), tr(lang, "find")));
        keys.insert(3, (k(Action::Remove), tr(lang, "remove")));
    }
    if app.tab == Tab::Library && !app.filtering {
        if !app.browser.is_songs() {
            keys[1] = (k(Action::Play), tr(lang, "open / play"));
            keys.insert(2, (k(Action::LibraryView), tr(lang, "view")));
            keys.insert(3, (k(Action::PlayGroup), tr(lang, "play group")));
            keys.insert(4, (k(Action::Queue), tr(lang, "queue")));
            keys.insert(5, (k(Action::Back), tr(lang, "back")));
            keys.insert(6, (k(Action::Search), tr(lang, "filter")));
        } else {
            keys.insert(2, (k(Action::Search), tr(lang, "filter")));
            keys.insert(3, (k(Action::Sort), tr(lang, "sort")));
            keys.insert(4, (k(Action::Mark), tr(lang, "mark")));
            keys.insert(5, (k(Action::MarkAll), tr(lang, "all")));
            keys.insert(6, (k(Action::Save), tr(lang, "save list")));
            keys.insert(2, (k(Action::LibraryView), tr(lang, "view")));
            keys.insert(3, (k(Action::PlayGroup), tr(lang, "play album")));
        }
    }
    if app.tab == Tab::Playlists {
        if app.open_playlist.is_none() {
            keys.insert(2, (k(Action::Mark), tr(lang, "new list")));
            keys.insert(3, (k(Action::OpenList), tr(lang, "open")));
            keys.insert(4, (k(Action::Remove), tr(lang, "delete")));
        } else {
            keys.insert(2, (k(Action::Mark), tr(lang, "add files")));
            keys.insert(3, (k(Action::TrackMark), tr(lang, "mark")));
            keys.insert(4, (k(Action::TrackMarkAll), tr(lang, "all")));
            keys.insert(5, (k(Action::Remove), tr(lang, "remove")));
            keys.insert(6, (k(Action::Cancel), tr(lang, "back")));
        }
    }
    if matches!(app.tab, Tab::Library | Tab::Playlists) {
        keys.insert(2, (k(Action::Rename), tr(lang, "rename")));
    }
    keys.insert(2, (k(Action::QueueView), tr(lang, "queue view")));
    keys.extend([
        (k(Action::Command), tr(lang, "command")),
        (k(Action::Theme), tr(lang, "theme")),
        (k(Action::Help), tr(lang, "help")),
        (k(Action::TabNext), tr(lang, "switch")),
        (k(Action::Quit), tr(lang, "quit")),
    ]);
    if compact {
        keys = vec![
            (k(Action::Play), tr(lang, "play")),
            (k(Action::Pause), tr(lang, "pause")),
            (
                format!("{}/{}", k(Action::VolumeUp), k(Action::VolumeDown)),
                tr(lang, "vol"),
            ),
            (k(Action::QueueView), tr(lang, "queue")),
        ];
        if app.tab == Tab::Library {
            keys.push((k(Action::LibraryView), tr(lang, "view")));
        }
        if matches!(app.tab, Tab::Library | Tab::Playlists) {
            keys.push((k(Action::Rename), tr(lang, "rename")));
        }
        keys.extend([
            (k(Action::TabNext), tr(lang, "tabs")),
            (k(Action::Help), tr(lang, "help")),
            (k(Action::Quit), tr(lang, "quit")),
        ]);
    }
    if app.tab == Tab::Library && app.filtering {
        keys = vec![
            (tr(lang, "type").to_string(), tr(lang, "filter")),
            ("↑↓".to_string(), tr(lang, "select")),
            ("⏎".to_string(), tr(lang, "keep")),
            ("esc".to_string(), tr(lang, "clear")),
            ("^c".to_string(), tr(lang, "quit")),
        ];
    }
    let item_width = |key: &str, label: &str| key.chars().count() + label.chars().count() + 3;
    let (quit_key, quit_label) = keys[keys.len() - 1].clone();
    let mut budget = (area.width as usize).saturating_sub(1 + item_width(&quit_key, quit_label));
    let mut shown = Vec::new();
    for (key, label) in &keys[..keys.len() - 1] {
        let width = item_width(key, label);
        if width > budget {
            break;
        }
        budget -= width;
        shown.push((key.clone(), *label));
    }
    shown.push((quit_key, quit_label));
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in shown {
        spans.push(Span::styled(
            key,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label}  "),
            Style::default().fg(theme.dim),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use crate::library::presentation::{
        browser::Browser, folder_picker::Picker, views::draw_library_browser,
    };
    use crate::radio::presentation::finder::Finder;
    use crate::shell::presentation::widgets::*;
    use ratatui::style::Color;
    use std::time::Duration;

    use super::*;
    use crate::playback::domain::spectrum::{FFT_SIZE, SampleTap, Spectrum};
    use crate::settings::presentation::theme::builtin_themes;

    #[test]
    fn command_help_entries_are_grouped_under_commands() {
        let mut section = "";
        for (key, description) in HELP {
            if key.is_empty() {
                section = description;
            }
            if key.starts_with(':') {
                assert_eq!(section, "Commands", "{key}");
            }
        }
    }

    #[test]
    fn spanish_headers_and_browser_keep_user_metadata_unchanged() {
        use crate::library::domain::library::LocalTrack;
        use ratatui::{Terminal, backend::TestBackend};
        let theme = &builtin_themes()[0];
        let mut terminal = Terminal::new(TestBackend::new(100, 1)).unwrap();
        terminal
            .draw(|frame| {
                let hits =
                    draw_compact_header(Language::Es, frame, frame.area(), Tab::Library, theme);
                assert_eq!(hits.len(), 4);
            })
            .unwrap();
        assert!(render_text(&terminal).contains("2 Listas"));
        let mut track: LocalTrack =
            crate::library::infrastructure::scanner::to_track("/music/Songs.wav".into());
        track.artist = "Songs".into();
        track.album = "Library".into();
        let tracks = vec![track];
        let mut browser = Browser::default();
        browser.set_view(
            crate::library::presentation::browser::View::Artists,
            &tracks,
        );
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal
            .draw(|frame| {
                draw_library_browser(
                    Language::Es,
                    frame,
                    frame.area(),
                    &mut browser,
                    None,
                    "",
                    theme,
                );
            })
            .unwrap();
        let rendered = render_text(&terminal);
        assert!(rendered.contains("Artistas"));
        assert!(rendered.contains("Songs"));
        assert!(rendered.contains("pista"));
        browser.open(&tracks);
        assert_eq!(browser.heading(Language::Es), "Songs");
        assert_eq!(browser.rows[0].display_title(Language::Es), "Library");
    }

    #[test]
    fn spectrum_draws() {
        let tap = SampleTap::new();
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                ((std::f32::consts::TAU * 220.0 * t).sin()
                    + (std::f32::consts::TAU * 3000.0 * t).sin())
                    * 0.4
            })
            .collect();
        tap.push(&samples);
        let mut spectrum = Spectrum::new();
        let config = VisualizerConfig {
            bar_width: 3,
            gap: 1,
            peaks: true,
            ..VisualizerConfig::default()
        };
        let area = Rect::new(0, 0, 60, 8);
        let bars = bar_count(area.width + 2, &config);
        for _ in 0..10 {
            spectrum.update(bars, &tap, true);
        }
        let themes = builtin_themes();
        let mut buf = Buffer::empty(area);
        SpectrumView {
            levels: spectrum.levels(),
            peaks: spectrum.peaks(),
            theme: &themes[0],
            bar_width: config.bar_width(),
            gap: config.gap(),
            show_peaks: true,
        }
        .render(area, &mut buf);
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        assert!(out.contains('█'));
    }

    #[test]
    fn folder_picker_draws() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let root = std::env::temp_dir().join(format!("crab-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Albums")).unwrap();
        std::fs::create_dir_all(root.join("Podcasts")).unwrap();
        let mut picker = Picker::new(root.clone());
        let themes = builtin_themes();
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        terminal
            .draw(|frame| draw_picker(Language::En, frame, frame.area(), &mut picker, &themes[0]))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..16 {
            for x in 0..80 {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        assert!(out.contains("Open folder"));
        assert!(out.contains("Albums"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn art_shows_through_the_selection_bar() {
        let themes = builtin_themes();
        let theme = &themes[0];
        let area = Rect::new(0, 0, 8, 3);
        let mut buf = Buffer::empty(area);
        for x in 0..8 {
            buf[(x, 1)].set_bg(theme.accent);
        }
        buf[(3, 1)].set_char('x');
        buf[(4, 2)].set_bg(Color::Rgb(1, 2, 3));
        let cell = |x, y| Cell {
            x,
            y,
            symbol: '█',
            depth: 0.5,
            rgb: None,
        };
        let cells = [cell(0, 0), cell(1, 1), cell(3, 1), cell(4, 2), cell(5, 1)];
        paint_art(&mut buf, &cells, area, theme, 0.35);
        assert_eq!(buf[(0, 0)].symbol(), "█");
        assert_eq!(buf[(1, 1)].symbol(), "█");
        assert_eq!(buf[(1, 1)].fg, theme.art_on_accent(0.35));
        assert_eq!(buf[(1, 1)].bg, theme.accent);
        assert_eq!(buf[(5, 1)].fg, theme.art_on_accent(0.35));
        assert_eq!(buf[(3, 1)].symbol(), "x");
        assert_eq!(buf[(4, 2)].symbol(), " ");
        assert_ne!(buf[(0, 0)].fg, buf[(1, 1)].fg);
    }

    #[test]
    fn finder_draws() {
        use crate::radio::infrastructure::search_repository::parse_results;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut finder = Finder::new(
            "ES".to_string(),
            crate::radio::application::search_stations_service::test_service(),
        );
        finder.results = parse_results(
            r#"[{"name":"Radio Uno","url_resolved":"https://a.example/uno.mp3","codec":"MP3","bitrate":128,"countrycode":"ES"},
                {"name":"A very long station name that must be cut so the details stay visible on screen","url_resolved":"https://a.example/dos.aac","codec":"AAC+","bitrate":64,"countrycode":"ES"}]"#,
        )
        .unwrap();
        finder.state.select(Some(0));
        finder.message = "2 stations".to_string();
        let themes = builtin_themes();
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| draw_finder(Language::En, frame, frame.area(), &mut finder, &themes[0]))
            .unwrap();
        let out = render_text(&terminal);
        assert!(out.contains("Find stations"));
        assert!(out.contains("Radio Uno"));
        assert!(out.contains("ES MP3 128k"));
        assert!(out.contains("ES AAC+ 64k"));
        assert!(out.contains('…'));
        assert!(out.contains("2 stations"));

        let mut small = Terminal::new(TestBackend::new(30, 8)).unwrap();
        small
            .draw(|frame| draw_finder(Language::En, frame, frame.area(), &mut finder, &themes[0]))
            .unwrap();
        assert!(!render_text(&small).contains("Find stations"));
    }

    #[test]
    fn song_rows_keep_titles_left_and_metadata_on_the_same_right_edge() {
        let theme = &builtin_themes()[0];
        let area = Rect::new(0, 0, 64, 2);
        let mut buffer = Buffer::empty(area);
        let long = "A long song title ".repeat(8);
        List::new(vec![
            aligned_row(64, "♪ ", "Short", "METADATA", Style::default(), theme),
            aligned_row(64, "  ", &long, "METADATA", Style::default(), theme),
        ])
        .render(area, &mut buffer);
        assert_eq!(buffer[(2, 0)].symbol(), "S");
        assert_eq!(buffer[(2, 1)].symbol(), "A");
        for row in 0..2 {
            let right: String = (56..64)
                .map(|column| buffer[(column, row)].symbol())
                .collect();
            assert_eq!(right, "METADATA");
        }
    }

    #[test]
    fn aligned_rows_fit_narrow_windows_and_wide_unicode_titles() {
        let theme = &builtin_themes()[0];
        for width in 0..80 {
            let area = Rect::new(0, 0, width, 1);
            let mut buffer = Buffer::empty(area);
            List::new(vec![aligned_row(
                width as usize,
                "♪ ",
                "東京の歌 — Café",
                "Artist",
                Style::default(),
                theme,
            )])
            .render(area, &mut buffer);
            if width >= 40 {
                assert_eq!(buffer[(width - 1, 0)].symbol(), "t");
            }
        }
        assert_eq!(fit_row_text("東京", 3), "東…");
        assert_eq!(fit_row_text("title", 0), "");
    }

    #[test]
    fn compact_header_keeps_tab_targets_inside_the_window() {
        use ratatui::{Terminal, backend::TestBackend};
        let theme = &builtin_themes()[0];
        for width in [1, 12, 29, 30, 40, 60, 100] {
            let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
            terminal
                .draw(|frame| {
                    let hits = draw_compact_header(
                        Language::En,
                        frame,
                        frame.area(),
                        Tab::Playlists,
                        theme,
                    );
                    assert_eq!(hits.len(), if width < 30 { 1 } else { 4 });
                    for (rect, _) in hits {
                        assert!(rect.width > 0);
                        assert!(rect.right() <= frame.area().right());
                        assert_eq!(rect.height, 1);
                    }
                })
                .unwrap();
            if width >= 30 {
                assert!(render_text(&terminal).contains("2 Lists"));
            }
        }
    }

    fn render_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn cover_panel_has_one_layout_for_all_sources_and_hides_when_narrow() {
        use ratatui::{Terminal, backend::TestBackend};
        let theme = &builtin_themes()[0];
        let mut terminal = Terminal::new(TestBackend::new(110, 18)).unwrap();
        let mut expected = None;
        for native in [false, true] {
            terminal
                .draw(|frame| {
                    let layout =
                        draw_cover_panel(Language::En, frame, frame.area(), None, native, theme);
                    if let Some(previous) = expected {
                        assert_eq!(layout, previous);
                    }
                    expected = Some(layout);
                    assert!(layout.1.is_some());
                    assert!(layout.0.width >= 35);
                })
                .unwrap();
            assert!(render_text(&terminal).contains("Album cover"));
        }
        let mut narrow = Terminal::new(TestBackend::new(60, 10)).unwrap();
        narrow
            .draw(|frame| {
                assert_eq!(
                    draw_cover_panel(Language::En, frame, frame.area(), None, false, theme),
                    (frame.area(), None)
                );
            })
            .unwrap();
        assert!(!render_text(&narrow).contains("Album cover"));
    }

    #[test]
    fn library_browser_draws_groups_albums_and_small_terminals() {
        use crate::library::presentation::browser::View;
        use ratatui::{Terminal, backend::TestBackend};
        let mut track =
            crate::library::infrastructure::scanner::to_track("/music/track.wav".into());
        track.artist = "Test artist".into();
        track.album = "Test album".into();
        track.title = "Test song".into();
        track.track = 1;
        track.duration = Some(Duration::from_secs(125));
        let tracks = vec![track];
        let mut browser = Browser::default();
        browser.set_view(View::Artists, &tracks);
        let theme = &builtin_themes()[0];
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        terminal
            .draw(|frame| {
                draw_library_browser(
                    Language::En,
                    frame,
                    frame.area(),
                    &mut browser,
                    None,
                    "",
                    theme,
                );
            })
            .unwrap();
        let text = render_text(&terminal);
        assert!(text.contains("Artists"));
        assert!(text.contains("Test artist"));
        assert!(text.contains("1 album"));
        browser.open(&tracks);
        browser.open(&tracks);
        terminal
            .draw(|frame| {
                draw_library_browser(
                    Language::En,
                    frame,
                    frame.area(),
                    &mut browser,
                    Some(&tracks[0].path),
                    "",
                    theme,
                );
            })
            .unwrap();
        let text = render_text(&terminal);
        assert!(text.contains("Test album"));
        assert!(text.contains("Test song"));
        assert!(text.contains("2:05"));
        assert!(!text.contains("Album cover"));
        assert!(text.contains('♪'));
        for width in [42, 1] {
            let mut small = Terminal::new(TestBackend::new(width, 4)).unwrap();
            small
                .draw(|frame| {
                    draw_library_browser(
                        Language::En,
                        frame,
                        frame.area(),
                        &mut browser,
                        None,
                        "",
                        theme,
                    );
                })
                .unwrap();
            assert!(!render_text(&small).contains("Album cover"));
        }
        browser.rebuild(&[]);
        terminal
            .draw(|frame| {
                draw_library_browser(
                    Language::En,
                    frame,
                    frame.area(),
                    &mut browser,
                    None,
                    "",
                    theme,
                );
            })
            .unwrap();
        assert!(render_text(&terminal).contains("No matching music"));
    }

    #[test]
    fn theme_overlays_draw() {
        use crate::settings::presentation::theme_editor::{Editor, ThemePicker};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let themes = builtin_themes();
        let config = Config::default();
        let mut picker = ThemePicker::new(2);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                draw_theme_picker(
                    Language::En,
                    frame,
                    frame.area(),
                    &mut picker,
                    &themes,
                    &config,
                    &themes[2],
                )
            })
            .unwrap();
        let out = render_text(&terminal);
        assert!(out.contains("synthwave"));
        assert!(out.contains("e edit"));
        assert!(out.contains("x delete"));
        picker.confirm = true;
        terminal
            .draw(|frame| {
                draw_theme_picker(
                    Language::En,
                    frame,
                    frame.area(),
                    &mut picker,
                    &themes,
                    &config,
                    &themes[2],
                )
            })
            .unwrap();
        assert!(render_text(&terminal).contains("Delete \"ocean\"?"));

        let editor = Editor::new(&themes[1], "mine".to_string(), 1);
        terminal
            .draw(|frame| draw_theme_editor(Language::En, frame, frame.area(), &editor, &themes[1]))
            .unwrap();
        let out = render_text(&terminal);
        assert!(out.contains("New theme: mine"));
        assert!(out.contains("accent"));
        assert!(out.contains("#ff2fa8"));
        assert!(out.contains("bar 3"));

        let mut editor = Editor::new(&themes[1], "mine".to_string(), 1);
        editor.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let picker = editor.color.as_ref().unwrap().picker;
        terminal
            .draw(|frame| {
                draw_theme_editor(Language::En, frame, frame.area(), &editor, &themes[1]);
                draw_color_popup(
                    Language::En,
                    frame,
                    frame.area(),
                    &picker,
                    "Color: accent",
                    None,
                    "help",
                    None,
                    &themes[1],
                )
            })
            .unwrap();
        let out = render_text(&terminal);
        println!("{out}");
        assert!(out.contains("Color: accent"));
        assert!(out.contains("Hue"));
        assert!(out.contains("Light"));
        assert!(out.contains("#ff2fa8"));

        terminal
            .draw(|frame| {
                draw_color_popup(
                    Language::En,
                    frame,
                    frame.area(),
                    &picker,
                    "New theme: pick an accent color",
                    Some("dark"),
                    "help",
                    None,
                    &themes[1],
                )
            })
            .unwrap();
        let out = render_text(&terminal);
        assert!(out.contains("Mode"));
        assert!(out.contains("dark"));
    }

    #[test]
    fn bar_count_config() {
        let config = VisualizerConfig {
            bar_width: 1,
            gap: 0,
            peaks: true,
            ..VisualizerConfig::default()
        };
        assert_eq!(bar_count(102, &config), 100);
        let wide = VisualizerConfig {
            bar_width: 3,
            gap: 1,
            peaks: true,
            ..VisualizerConfig::default()
        };
        assert_eq!(bar_count(82, &wide), 20);
    }
}
