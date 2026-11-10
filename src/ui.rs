use crate::app::{Active, App, SpotifyStatus, Tab};
use crate::art::{self, Art, Cell};
use crate::config::Config;
use crate::config::{VisualizerConfig, VizStyle};
use crate::finder::{Finder, Focus};
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

    match app.tab {
        Tab::Library => draw_library(frame, body, app, &theme),
        Tab::Radio => draw_radio(frame, body, app, &theme),
        Tab::Spotify => draw_spotify(frame, body, app, &theme),
    }
    draw_art(frame, body, app, &theme);
    draw_spectrum(frame, viz, app, &theme);
    draw_now_playing(frame, now, app, &theme);
    draw_footer(frame, footer, app, &theme);
    if let Some(picker) = app.picker.as_mut() {
        draw_picker(frame, area, picker, &theme);
    }
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
    let strength = app.config.art_strength();
    let base_bg = theme.background.unwrap_or(Color::Reset);
    let buf = frame.buffer_mut();
    for cell in cells {
        let x = inner.x + cell.x;
        let y = inner.y + cell.y;
        let target = &buf[(x, y)];
        if target.symbol() != " " || target.style().bg.unwrap_or(Color::Reset) != base_bg {
            continue;
        }
        let space_left = x == inner.x || buf[(x - 1, y)].symbol() == " ";
        let space_right = x + 1 >= inner.x + inner.width || buf[(x + 1, y)].symbol() == " ";
        if !(space_left || space_right) {
            continue;
        }
        buf[(x, y)]
            .set_char(cell.symbol)
            .set_fg(theme.art_color(1.0 - cell.depth, strength));
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
    let mut block = panel(theme, "Open folder".to_string());
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
                format!("▸ {name}")
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

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let wide = area.width >= 72;
    let [logo, tabs, theme_name] = Layout::horizontal([
        Constraint::Length(if wide { 26 } else { 10 }),
        Constraint::Min(10),
        Constraint::Length(theme.name.chars().count() as u16 + 4),
    ])
    .areas(area);
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
    let folders = app.roots_label();
    let title = if app.scanning {
        format!("Library - {folders} - scanning")
    } else {
        format!("Library - {folders} - {} tracks", app.tracks.len())
    };
    let items: Vec<ListItem> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let playing = app.active == Active::Local(i);
            let marker = if playing { "♪ " } else { "  " };
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
                    format!("   {}", track.folder),
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

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
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
        Active::Local(_) => (app.title.clone(), app.artist.clone()),
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
    if !artist.is_empty() {
        spans.push(Span::styled(
            format!("  -  {artist}"),
            Style::default().fg(theme.dim),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), top);

    let [progress, volume] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(22)]).areas(bottom);

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
    let mut keys = vec![
        ("↑↓", "select"),
        ("⏎", "play"),
        ("space", "pause"),
        ("n/p", "next/prev"),
        ("←→", "seek"),
        ("+/-", "volume"),
        ("m", "mute"),
        ("s", if app.shuffle { "shuffle on" } else { "shuffle" }),
        ("o", "folder"),
        ("v", "viz"),
    ];
    if app.tab == Tab::Radio {
        keys.insert(2, ("/", "find"));
        keys.insert(3, ("x", "remove"));
    }
    keys.extend([("t", "theme"), ("tab", "switch"), ("q", "quit")]);
    let item_width = |key: &str, label: &str| key.chars().count() + label.chars().count() + 3;
    let (quit_key, quit_label) = keys[keys.len() - 1];
    let mut budget = (area.width as usize).saturating_sub(1 + item_width(quit_key, quit_label));
    let mut shown = Vec::new();
    for (key, label) in &keys[..keys.len() - 1] {
        let width = item_width(key, label);
        if width > budget {
            break;
        }
        budget -= width;
        shown.push((*key, *label));
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
