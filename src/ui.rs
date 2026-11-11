use crate::app::{Active, App, Repeat, SpotifyStatus, Tab};
use crate::art::{self, Art, Cell};
use crate::clock;
use crate::config::Config;
use crate::config::{VisualizerConfig, VizStyle};
use crate::cover::Cover;
use crate::finder::{Finder, Focus};
use crate::keys::Action;
use crate::library_browser::{Browser, duration_label};
use crate::picker::Picker;
use crate::spotify;
use crate::theme::{Theme, from_hsl, to_color, to_hex};
use crate::theme_editor::{ColorPicker, Editor, ThemePicker};
use crate::viz::{CurveView, MirrorView, WaterfallView, WaveView};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, LineGauge, List, ListItem, Padding, Paragraph, Tabs, Widget,
    Wrap,
};
use std::time::Duration;

const TABS_WIDTH: u16 = 50;
const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

pub fn bar_count(width: u16, config: &VisualizerConfig) -> usize {
    let inner = width.saturating_sub(2) as usize;
    let gap = config.gap() as usize;
    let step = config.bar_width() as usize + gap;
    ((inner + gap) / step).max(1)
}

fn format_time(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

fn panel(theme: &Theme, title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
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
    let theme = app.theme().clone();
    let area = frame.area();
    if let Some(background) = theme.background {
        frame.render_widget(
            Block::default().style(Style::default().bg(background)),
            area,
        );
    }
    let viz_height = (area.height / 3).clamp(7, 18);
    let [header, body, viz, now, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(viz_height),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header, app, &theme);
    app.hit.tabs = locate_tabs(frame.buffer_mut(), header);
    app.cover_area = None;
    let body = if matches!(app.tab, Tab::Library | Tab::Playlists | Tab::Spotify) {
        let (content, image) = draw_cover_panel(
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
    if app.tab != Tab::Library || app.browser.is_songs() {
        draw_art(frame, body, app, &theme);
    }
    draw_spectrum(frame, viz, app, &theme);
    draw_now_playing(frame, now, app, &theme);
    draw_footer(frame, footer, app, &theme);
    if let Some(picker) = app.picker.as_mut() {
        draw_picker(frame, area, picker, &theme);
    }
    draw_name_prompt(frame, area, app, &theme);
    draw_queue_view(frame, area, app, &theme);
    draw_help(frame, area, app, &theme);
    draw_lyrics(frame, area, app, &theme);
    if let Some(finder) = app.finder.as_mut() {
        draw_finder(frame, area, finder, &theme);
    }
    if let Some(picker) = app.theme_picker.as_mut() {
        draw_theme_picker(frame, area, picker, &app.themes, &app.config, &theme);
    }
    if let Some(editor) = &app.theme_editor {
        draw_theme_editor(frame, area, editor, &theme);
        if let Some(editing) = &editor.color {
            let title = format!("Color: {}", editor.label(editor.selected));
            let help = "←→ hue  ↑↓ lightness  PgUp/PgDn saturation  ⏎ ok  # hex  esc cancel";
            let rows = editor.rows() as u16;
            let editor_top = area.y
                + area
                    .height
                    .saturating_sub((rows + 6).min(area.height.saturating_sub(2)))
                    / 2;
            let selected_row = editor_top + 2 + editor.selected as u16;
            draw_color_popup(
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
        let mode = if generator.dark { "dark" } else { "light" };
        let help =
            "←→ hue  ↑↓ lightness  PgUp/PgDn saturation  m dark/light  ⏎ continue  esc cancel";
        draw_color_popup(
            frame,
            area,
            &generator.picker,
            "New theme: pick an accent color",
            Some(mode),
            help,
            None,
            &theme,
        );
    }
}

fn contrast_for(color: (u8, u8, u8)) -> Color {
    let luma = 0.2126 * color.0 as f32 + 0.7152 * color.1 as f32 + 0.0722 * color.2 as f32;
    if luma > 140.0 {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    }
}

fn bar_line(
    label: &str,
    width: usize,
    position: f32,
    theme: &Theme,
    color_at: impl Fn(f32) -> (u8, u8, u8),
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{label:<6}"),
        Style::default().fg(theme.dim),
    )];
    let last = width.saturating_sub(1).max(1);
    let marker = (position.clamp(0.0, 1.0) * last as f32).round() as usize;
    for i in 0..width {
        let color = color_at(i as f32 / last as f32);
        if i == marker {
            spans.push(Span::styled(
                "┃",
                Style::default().fg(contrast_for(color)).bg(to_color(color)),
            ));
        } else {
            spans.push(Span::styled("█", Style::default().fg(to_color(color))));
        }
    }
    Line::from(spans)
}

#[allow(clippy::too_many_arguments)]
fn draw_color_popup(
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
        Paragraph::new(bar_line("Hue", bar_width, h / 360.0, theme, |t| {
            from_hsl(t * 360.0, s, l)
        })),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(bar_line("Sat", bar_width, s, theme, |t| from_hsl(h, t, l))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(bar_line("Light", bar_width, l, theme, |t| {
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
                Span::styled("Mode  ", Style::default().fg(theme.dim)),
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

fn swatch_color(color: (u8, u8, u8), cells: usize) -> Vec<Span<'static>> {
    vec![Span::styled(
        "█".repeat(cells),
        Style::default().fg(to_color(color)),
    )]
}

fn overlay_block(frame: &mut Frame, rect: Rect, theme: &Theme, title: String) -> Rect {
    frame.render_widget(Clear, rect);
    let mut block = panel(theme, title);
    if let Some(background) = theme.background {
        block = block.style(Style::default().bg(background));
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn swatch(theme: &Theme, cells: usize) -> Vec<Span<'static>> {
    (0..cells)
        .map(|i| {
            let t = i as f32 / cells.saturating_sub(1).max(1) as f32;
            Span::styled("█", Style::default().fg(theme.gradient(t)))
        })
        .collect()
}

fn draw_theme_picker(
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
    let inner = overlay_block(frame, rect, theme, "Theme".to_string());
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
                spans.push(Span::styled("  custom", Style::default().fg(tag)));
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
            format!("Delete \"{selected_name}\"? y to confirm, any other key cancels"),
            Style::default().fg(theme.error),
        )
    } else if let Some(notice) = &picker.notice {
        Line::styled(notice.clone(), Style::default().fg(theme.error))
    } else {
        Line::styled(
            "↑↓ preview  ⏎ use  e edit  n new  x delete  esc cancel",
            Style::default().fg(theme.dim),
        )
    };
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: true }), help_area);
}

fn draw_theme_editor(frame: &mut Frame, area: Rect, editor: &Editor, theme: &Theme) {
    if area.width < 34 || area.height < 11 {
        return;
    }
    let wanted = editor.rows() as u16 + 6;
    let rect = centered(area, 60, wanted.min(area.height.saturating_sub(2)));
    let inner = overlay_block(frame, rect, theme, format!("New theme: {}", editor.name));
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
                        Style::default().fg(crate::theme::to_color(rgb)),
                    ));
                    spans.push(Span::styled(to_hex(rgb), Style::default().fg(theme.dim)));
                }
                None => spans.push(Span::styled("none", Style::default().fg(theme.dim))),
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
            Span::styled("Save as: ", Style::default().fg(theme.dim)),
            Span::styled(name.clone(), Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.accent)),
        ])
    } else if let Some(input) = &editor.input {
        Line::from(vec![
            Span::styled("Hex: ", Style::default().fg(theme.dim)),
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
        "⏎ save to config.toml  esc back"
    } else if editor.input.is_some() {
        "type a hex color  ⏎ apply  esc cancel"
    } else {
        "⏎ color  # hex  d clear  +/- bars  s save  esc discard"
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

fn cover_color(rgb: [u8; 3], base: Color, strength: f32) -> Color {
    let (br, bg, bb) = match base {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (16.0, 16.0, 20.0),
    };
    let t = (strength * 1.6).clamp(0.12, 0.85);
    let mix = |from: f32, to: u8| (from + (to as f32 - from) * t).round() as u8;
    Color::Rgb(mix(br, rgb[0]), mix(bg, rgb[1]), mix(bb, rgb[2]))
}

fn paint_art(buf: &mut Buffer, cells: &[Cell], inner: Rect, theme: &Theme, strength: f32) {
    let base_bg = theme.background.unwrap_or(Color::Reset);
    for cell in cells {
        let x = inner.x + cell.x;
        let y = inner.y + cell.y;
        let target = &buf[(x, y)];
        let bg = target.style().bg.unwrap_or(Color::Reset);
        let blank = target.symbol() == " ";
        let on_bar = blank && bg == theme.accent;
        if !blank || (bg != base_bg && !on_bar) {
            continue;
        }
        let space_left = x == inner.x || buf[(x - 1, y)].symbol() == " ";
        let space_right = x + 1 >= inner.x + inner.width || buf[(x + 1, y)].symbol() == " ";
        if !(space_left || space_right) {
            continue;
        }
        if let Some((top, bottom)) = cell.rgb {
            if on_bar {
                continue;
            }
            let (fg, bg) = (
                cover_color(top, base_bg, strength),
                cover_color(bottom, base_bg, strength),
            );
            buf[(x, y)].set_char(cell.symbol).set_fg(fg).set_bg(bg);
            continue;
        }
        let color = if on_bar {
            theme.art_on_accent(strength)
        } else {
            theme.art_color(1.0 - cell.depth, strength)
        };
        buf[(x, y)].set_char(cell.symbol).set_fg(color);
    }
}

fn draw_picker(frame: &mut Frame, area: Rect, picker: &mut Picker, theme: &Theme) {
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
            "Add to playlist".to_string()
        } else {
            "Open folder".to_string()
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
            Span::styled("Go to: ", Style::default().fg(theme.dim)),
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
            "⏎ open folder or add file  space mark  a add marked  A add all here  ⌫ up  ~ home  / type path  esc done",
            Style::default().fg(theme.dim),
        )
    } else {
        Line::styled(
            "⏎ open  ⌫ up  space use this folder  a add  ~ home  / type path  . hidden  esc cancel",
            Style::default().fg(theme.dim),
        )
    };
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: true }), help_area);
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn field_line(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };
    let mut spans = vec![
        Span::styled(format!("{label:<9}"), label_style),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ];
    if focused {
        spans.push(Span::styled("█", Style::default().fg(theme.accent)));
    }
    Line::from(spans)
}

fn draw_finder(frame: &mut Frame, area: Rect, finder: &mut Finder, theme: &Theme) {
    let width = area.width.saturating_sub(4).min(78);
    let height = area.height.saturating_sub(2).min(24);
    if width < 36 || height < 10 {
        return;
    }
    let rect = centered(area, width, height);
    let inner = overlay_block(frame, rect, theme, "Find stations".to_string());
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
            "Name",
            &finder.name,
            finder.focus == Focus::Name,
            theme,
        )),
        name_area,
    );
    frame.render_widget(
        Paragraph::new(field_line(
            "Country",
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
        Focus::Results => "↑↓ select  ⏎ save and play  / new search  tab fields  esc close",
        _ => "type to search  ⏎ search  ↓ results  tab next field  esc close",
    };
    frame.render_widget(
        Paragraph::new(Line::styled(help, Style::default().fg(theme.dim)))
            .wrap(Wrap { trim: true }),
        help_area,
    );
}

fn locate_tabs(buf: &Buffer, header: Rect) -> Vec<(Rect, Tab)> {
    let row: Vec<&str> = (header.x..header.x + header.width)
        .map(|x| buf[(x, header.y)].symbol())
        .collect();
    let mut found = Vec::new();
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let label = format!("{} {}", i + 1, tab.label());
        let chars: Vec<String> = label.chars().map(|c| c.to_string()).collect();
        let start = (0..row.len().saturating_sub(chars.len() - 1))
            .find(|&at| chars.iter().enumerate().all(|(k, c)| row[at + k] == c));
        if let Some(at) = start {
            let x = header.x + at as u16;
            found.push((
                Rect::new(x.saturating_sub(1), header.y, label.len() as u16 + 2, 1),
                *tab,
            ));
        }
    }
    found
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
        .map(|(i, tab)| Line::from(format!(" {} {} ", i + 1, tab.label())))
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

fn highlight(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.on_accent)
        .bg(theme.accent)
        .add_modifier(Modifier::BOLD)
}

fn draw_library(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    if !app.browser.is_songs() {
        let playing = app.current_track_path().cloned();
        let filter = if app.filtering {
            format!(" - filter: {}█", app.filter)
        } else if app.filter.is_empty() {
            String::new()
        } else {
            format!(" - filter: {}", app.filter)
        };
        let context = format!(
            "{}{}{}",
            filter,
            if app.only_favorites {
                " - favorites"
            } else {
                ""
            },
            if app.scanning { " - scanning" } else { "" }
        );
        app.hit.body = draw_library_browser(
            frame,
            area,
            &mut app.browser,
            app.browser_cover.as_ref(),
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
        format!(" - {} marked", app.marked.len())
    };
    let favorites = if app.only_favorites {
        " - favorites"
    } else {
        ""
    };
    let filter = if app.filtering {
        format!(" - filter: {}█", app.filter)
    } else if app.filter.is_empty() {
        String::new()
    } else {
        format!(" - filter: {}", app.filter)
    };
    let title = if app.scanning {
        format!("Library - {folders} - scanning")
    } else {
        format!(
            "Library - {folders} - {} tracks{favorites}{marked}{filter}",
            app.tracks.len()
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
            } else if app.favorites.contains(&track.path) {
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
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.accent)),
                Span::styled(track.title.clone(), title_style),
                Span::styled(
                    format!("   {}", track.detail()),
                    Style::default().fg(theme.dim),
                ),
            ]))
        })
        .collect();

    if items.is_empty() {
        let message = if app.scanning {
            "Scanning for audio files..."
        } else {
            "No audio files found. Press o to choose a folder"
        };
        let message = if app.only_favorites && app.filter.is_empty() {
            "No favorites yet, press f on a track"
        } else if app.filter.is_empty() {
            message
        } else {
            "Nothing matches the filter"
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

fn draw_cover_panel(
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
    let block = panel(theme, "Album cover".into());
    let inner = block.inner(art);
    frame.render_widget(block, art);
    if let Some(cover) = cover {
        if !native {
            let cells = cover.cells(inner.width, inner.height);
            paint_art(frame.buffer_mut(), &cells, inner, theme, 1.0);
        }
    } else {
        frame.render_widget(
            Paragraph::new("No cover art")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.dim)),
            inner,
        );
    }
    (list, Some(inner))
}

fn draw_library_browser(
    frame: &mut Frame,
    area: Rect,
    browser: &mut Browser,
    _cover: Option<&Cover>,
    playing: Option<&std::path::PathBuf>,
    context: &str,
    theme: &Theme,
) -> Rect {
    let list_area = area;
    let title = if browser.is_album() {
        let duration = duration_label(browser.rows.iter().flat_map(|row| row.tracks.iter()));
        format!(
            "{} - {} track{} · {}{context}",
            browser.title,
            browser.rows.len(),
            if browser.rows.len() == 1 { "" } else { "s" },
            duration
        )
    } else {
        format!(
            "Library - {} - {} groups{context}",
            browser.title,
            browser.rows.len()
        )
    };
    if browser.rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching music. Clear the filter or press o to choose a folder.")
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
                ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(theme.accent)),
                    Span::styled(
                        row.title.clone(),
                        Style::default().fg(if active { theme.accent } else { theme.text }),
                    ),
                    Span::styled(format!("   {}", row.detail), Style::default().fg(theme.dim)),
                ]))
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

fn draw_playlists(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let row_width = area.width.saturating_sub(4) as usize;
    let (title, items, message): (String, Vec<ListItem>, &str) = match app.open_playlist {
        None => {
            let items = app
                .playlists
                .iter()
                .map(|list| {
                    let playing = app.playing_playlist(&list.name);
                    let count = format!("{} tracks", list.tracks.len());
                    let name_width = row_width.saturating_sub(count.chars().count() + 4);
                    let name = truncate(&list.name, name_width);
                    let pad = row_width
                        .saturating_sub(name.chars().count() + count.chars().count() + 2)
                        .max(1);
                    let name_style = if playing {
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if playing { "♪ " } else { "≡ " },
                            Style::default().fg(theme.accent),
                        ),
                        Span::styled(name, name_style),
                        Span::raw(" ".repeat(pad)),
                        Span::styled(count, Style::default().fg(theme.dim)),
                    ]))
                })
                .collect();
            (
                format!("Playlists - {}", app.playlists.len()),
                items,
                "No playlists yet. Press a to create one, or mark tracks in Library and press w",
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
                    let track = app
                        .meta(path)
                        .cloned()
                        .unwrap_or_else(|| crate::library::to_track(path.clone()));
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
                    ListItem::new(Line::from(vec![
                        Span::styled(marker, Style::default().fg(theme.accent)),
                        Span::styled(track.title.clone(), title_style),
                        Span::styled(
                            format!("   {}", track.detail()),
                            Style::default().fg(theme.dim),
                        ),
                    ]))
                })
                .collect();
            let marked = if app.marked_tracks.is_empty() {
                String::new()
            } else {
                format!(" - {} marked", app.marked_tracks.len())
            };
            (
                format!("Playlist - {name} - {} tracks{marked}", tracks.len()),
                items,
                "Empty playlist. Press a to add files or folders",
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

fn draw_queue_view(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
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
            format!("Up next - {}", app.up_next.len()),
        );
        let [list_area, help_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        if app.up_next.is_empty() {
            frame.render_widget(
                Paragraph::new("Nothing queued. Press Q on a track to queue it")
                    .style(Style::default().fg(theme.dim)),
                list_area,
            );
        } else {
            let items: Vec<ListItem> = app
                .up_next
                .iter()
                .map(|track| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            truncate(&track.title, list_area.width as usize / 2),
                            Style::default().fg(theme.text),
                        ),
                        Span::styled(
                            format!("   {}", track.artist_or_folder()),
                            Style::default().fg(theme.dim),
                        ),
                    ]))
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
                "⏎ play now  x remove  X clear  esc close",
                Style::default().fg(theme.dim),
            )),
            help_area,
        );
    }
    app.queue_view = Some(state);
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
        && let Some(action) = crate::keys::action_by_name(name)
    {
        return app.keys.all(action);
    }
    tokens
        .iter()
        .map(|token| match token.strip_prefix('@') {
            Some(name) => crate::keys::action_by_name(name)
                .map_or_else(|| (*token).to_string(), |a| app.keys.first(a)),
            None => (*token).to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_help(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let Some(offset) = app.help else {
        return;
    };
    let width = area.width.saturating_sub(4).min(90);
    let height = area.height.saturating_sub(2).min(HELP.len() as u16 + 3);
    if width < 40 || height < 8 {
        return;
    }
    let rect = centered(area, width, height);
    let inner = overlay_block(frame, rect, theme, "Keys".to_string());
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
                    text.to_string(),
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
                    Span::styled(text.to_string(), Style::default().fg(theme.text)),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list_area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "up/down scroll  esc close",
            Style::default().fg(theme.dim),
        )),
        help_area,
    );
}

fn draw_lyrics(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
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
    let title = format!("Lyrics · {}", app.title);
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
        "synced  esc close"
    } else {
        "up/down scroll  esc close"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(theme.dim))),
        help_area,
    );
}

fn draw_name_prompt(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
    let inner = overlay_block(frame, rect, theme, prompt.title.to_string());
    let [field_area, list_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(field_line(prompt.label, &prompt.input, true, theme)),
        field_area,
    );
    let current = crate::playlist::find(&app.playlists, &prompt.input);
    let names_only = matches!(
        prompt.kind,
        crate::app::PromptKind::SaveMarked | crate::app::PromptKind::NewList
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
        "⏎ ok  esc cancel"
    } else {
        "⏎ ok  ↑↓ existing list  esc cancel"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(help, Style::default().fg(theme.dim))),
        help_area,
    );
}

fn draw_radio(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
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
                format!("Radio - {} stations", app.stations.len()),
            ))
            .highlight_style(highlight(theme)),
        area,
        &mut app.radio_state,
    );
}

fn draw_spotify(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    match &app.sp.status {
        SpotifyStatus::LoggedOut if !spotify::ENABLED => {
            lines.push(Line::styled(
                "Spotify support is not included in this build",
                Style::default()
                    .fg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Rebuild with default features to enable it: cargo build --release",
                Style::default().fg(theme.dim),
            ));
        }
        SpotifyStatus::LoggedOut => {
            lines.push(Line::styled(
                "Not signed in to Spotify",
                Style::default()
                    .fg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Close CRAB and run:  crab login",
                Style::default().fg(theme.text),
            ));
            lines.push(Line::styled(
                "A Spotify Premium account is required.",
                Style::default().fg(theme.dim),
            ));
        }
        SpotifyStatus::Connecting => {
            lines.push(Line::styled(
                "Connecting to Spotify...",
                Style::default().fg(theme.secondary),
            ));
        }
        SpotifyStatus::Failed(message) => {
            lines.push(Line::styled(
                "Spotify connection failed",
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
                    "CRAB is available as a Spotify Connect device",
                    Style::default().fg(theme.text),
                ),
            ]));
            lines.push(Line::styled(
                "Open Spotify on your phone or desktop and pick \"CRAB\" from the devices menu.",
                Style::default().fg(theme.dim),
            ));
            lines.push(Line::raw(""));
            if app.sp.title.is_empty() {
                lines.push(Line::styled(
                    "Nothing playing yet",
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
            "Unofficial integration, not affiliated with or endorsed by Spotify.",
            Style::default().fg(theme.dim),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(theme, "Spotify".to_string()).padding(Padding::new(2, 2, 1, 0))),
        area,
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

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let block = panel(theme, "Now playing".to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [top, bottom] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

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
        Active::None => (String::from("Nothing playing"), String::new()),
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
    ) && !app.album.is_empty()
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
        badges.push("shuffle".to_string());
    }
    if app.repeat != Repeat::Off {
        badges.push(format!("repeat {}", app.repeat.label()));
    }
    if !app.up_next.is_empty() {
        badges.push(format!("{} up next", app.up_next.len()));
    }
    if app.config.audio.eq_gains().iter().any(|g| g.abs() >= 0.05) {
        badges.push(format!("eq {}", app.config.audio.eq));
    }
    if let Some(stats) = app.stats_badge() {
        badges.push(stats);
    }
    if app.visual_active() {
        badges.push("visual".to_string());
    }
    if let Some(left) = app.sleep_left() {
        badges.push(format!("sleep {}m", left.as_secs().div_ceil(60)));
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

    let [progress, volume] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(22)]).areas(bottom);
    app.hit.progress = progress;

    let position = app.position();
    let (ratio, label) = match app.total() {
        Some(total) if !total.is_zero() => (
            (position.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0),
            format!("{} / {}", format_time(position), format_time(total)),
        ),
        _ => match app.active {
            Active::Radio(_) if !app.loading => (0.0, format!("LIVE {}", format_time(position))),
            _ => (0.0, String::from("--:--")),
        },
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

    let level = if app.muted { 0.0 } else { app.volume() };
    let cells = 8usize;
    let filled = (level * cells as f32).round() as usize;
    let mut volume_spans = vec![Span::styled("  VOL ", Style::default().fg(theme.dim))];
    volume_spans.push(Span::styled(
        "█".repeat(filled),
        Style::default().fg(theme.secondary),
    ));
    volume_spans.push(Span::styled(
        "░".repeat(cells - filled),
        Style::default().fg(theme.border),
    ));
    let readout = if app.muted {
        " MUTE".to_string()
    } else {
        format!(" {:>3}%", (app.volume() * 100.0).round() as u32)
    };
    volume_spans.push(Span::styled(readout, Style::default().fg(theme.dim)));
    frame.render_widget(Paragraph::new(Line::from(volume_spans)), volume);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
        (format!("{}{}", k(Action::Up), k(Action::Down)), "select"),
        (k(Action::Play), "play"),
        (k(Action::Pause), "pause"),
        (
            format!("{}/{}", k(Action::Next), k(Action::Prev)),
            "next/prev",
        ),
        (
            format!("{}{}", k(Action::SeekBack), k(Action::SeekForward)),
            "seek",
        ),
        (
            format!("{}/{}", k(Action::VolumeUp), k(Action::VolumeDown)),
            "volume",
        ),
        (k(Action::Mute), "mute"),
        (
            k(Action::Shuffle),
            if app.shuffle { "shuffle on" } else { "shuffle" },
        ),
        (k(Action::Repeat), "repeat"),
        (k(Action::Queue), "queue"),
        (k(Action::Visualizer), "viz"),
        (k(Action::Clock), "clock"),
        (k(Action::Folder), "folder"),
    ];
    if app.tab == Tab::Radio {
        keys.insert(2, (k(Action::Search), "find"));
        keys.insert(3, (k(Action::Remove), "remove"));
    }
    if app.tab == Tab::Library && !app.filtering {
        if !app.browser.is_songs() {
            keys[1] = (k(Action::Play), "open / play");
            keys.insert(2, (k(Action::LibraryView), "view"));
            keys.insert(3, (k(Action::PlayGroup), "play group"));
            keys.insert(4, (k(Action::Queue), "queue"));
            keys.insert(5, (k(Action::Back), "back"));
            keys.insert(6, (k(Action::Search), "filter"));
        } else {
            keys.insert(2, (k(Action::Search), "filter"));
            keys.insert(3, (k(Action::Sort), "sort"));
            keys.insert(4, (k(Action::Mark), "mark"));
            keys.insert(5, (k(Action::MarkAll), "all"));
            keys.insert(6, (k(Action::Save), "save list"));
            keys.insert(2, (k(Action::LibraryView), "view"));
            keys.insert(3, (k(Action::PlayGroup), "play album"));
        }
    }
    if app.tab == Tab::Playlists {
        if app.open_playlist.is_none() {
            keys.insert(2, (k(Action::Mark), "new list"));
            keys.insert(3, (k(Action::OpenList), "open"));
            keys.insert(4, (k(Action::Remove), "delete"));
        } else {
            keys.insert(2, (k(Action::Mark), "add files"));
            keys.insert(3, (k(Action::TrackMark), "mark"));
            keys.insert(4, (k(Action::TrackMarkAll), "all"));
            keys.insert(5, (k(Action::Remove), "remove"));
            keys.insert(6, (k(Action::Cancel), "back"));
        }
    }
    keys.extend([
        (k(Action::Command), "command"),
        (k(Action::Theme), "theme"),
        (k(Action::Help), "help"),
        (k(Action::TabNext), "switch"),
        (k(Action::Quit), "quit"),
    ]);
    if app.tab == Tab::Library && app.filtering {
        keys = vec![
            ("type".to_string(), "filter"),
            ("↑↓".to_string(), "select"),
            ("⏎".to_string(), "keep"),
            ("esc".to_string(), "clear"),
            ("^c".to_string(), "quit"),
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
    use super::*;
    use crate::analyzer::{FFT_SIZE, SampleTap, Spectrum};
    use crate::theme::builtin_themes;

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
            .draw(|frame| draw_picker(frame, frame.area(), &mut picker, &themes[0]))
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
        use crate::finder::parse_results;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut finder = Finder::new("ES".to_string());
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
            .draw(|frame| draw_finder(frame, frame.area(), &mut finder, &themes[0]))
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
            .draw(|frame| draw_finder(frame, frame.area(), &mut finder, &themes[0]))
            .unwrap();
        assert!(!render_text(&small).contains("Find stations"));
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
                    let layout = draw_cover_panel(frame, frame.area(), None, native, theme);
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
                    draw_cover_panel(frame, frame.area(), None, false, theme),
                    (frame.area(), None)
                );
            })
            .unwrap();
        assert!(!render_text(&narrow).contains("Album cover"));
    }

    #[test]
    fn library_browser_draws_groups_albums_and_small_terminals() {
        use crate::library_browser::View;
        use ratatui::{Terminal, backend::TestBackend};
        let mut track = crate::library::to_track("/music/track.wav".into());
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
                draw_library_browser(frame, frame.area(), &mut browser, None, None, "", theme);
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
                    frame,
                    frame.area(),
                    &mut browser,
                    None,
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
                    draw_library_browser(frame, frame.area(), &mut browser, None, None, "", theme);
                })
                .unwrap();
            assert!(!render_text(&small).contains("Album cover"));
        }
        browser.rebuild(&[]);
        terminal
            .draw(|frame| {
                draw_library_browser(frame, frame.area(), &mut browser, None, None, "", theme);
            })
            .unwrap();
        assert!(render_text(&terminal).contains("No matching music"));
    }

    #[test]
    fn theme_overlays_draw() {
        use crate::theme_editor::{Editor, ThemePicker};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let themes = builtin_themes();
        let config = Config::default();
        let mut picker = ThemePicker::new(2);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                draw_theme_picker(
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
            .draw(|frame| draw_theme_editor(frame, frame.area(), &editor, &themes[1]))
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
                draw_theme_editor(frame, frame.area(), &editor, &themes[1]);
                draw_color_popup(
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
