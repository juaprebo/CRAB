use crate::playback::domain::artwork::Cell;
use crate::settings::presentation::theme::{Theme, to_color};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, ListItem},
};
use std::time::Duration;

pub(crate) fn format_time(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

pub(crate) fn panel(theme: &Theme, title: String) -> Block<'static> {
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

pub(crate) fn contrast_for(color: (u8, u8, u8)) -> Color {
    let luma = 0.2126 * color.0 as f32 + 0.7152 * color.1 as f32 + 0.0722 * color.2 as f32;
    if luma > 140.0 {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    }
}

pub(crate) fn bar_line(
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

pub(crate) fn swatch_color(color: (u8, u8, u8), cells: usize) -> Vec<Span<'static>> {
    vec![Span::styled(
        "█".repeat(cells),
        Style::default().fg(to_color(color)),
    )]
}

pub(crate) fn overlay_block(frame: &mut Frame, rect: Rect, theme: &Theme, title: String) -> Rect {
    frame.render_widget(Clear, rect);
    let mut block = panel(theme, title);
    if let Some(background) = theme.background {
        block = block.style(Style::default().bg(background));
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub(crate) fn swatch(theme: &Theme, cells: usize) -> Vec<Span<'static>> {
    (0..cells)
        .map(|i| {
            let t = i as f32 / cells.saturating_sub(1).max(1) as f32;
            Span::styled("█", Style::default().fg(theme.gradient(t)))
        })
        .collect()
}

pub(crate) fn cover_color(rgb: [u8; 3], base: Color, strength: f32) -> Color {
    let (br, bg, bb) = match base {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (16.0, 16.0, 20.0),
    };
    let t = (strength * 1.6).clamp(0.12, 0.85);
    let mix = |from: f32, to: u8| (from + (to as f32 - from) * t).round() as u8;
    Color::Rgb(mix(br, rgb[0]), mix(bg, rgb[1]), mix(bb, rgb[2]))
}

pub(crate) fn paint_art(
    buf: &mut Buffer,
    cells: &[Cell],
    inner: Rect,
    theme: &Theme,
    strength: f32,
) {
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

pub(crate) fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn field_line(label: &str, value: &str, focused: bool, theme: &Theme) -> Line<'static> {
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

pub(crate) fn highlight(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.on_accent)
        .bg(theme.accent)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn fit_row_text(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Line::raw(text).width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        out.push(ch);
        if Line::raw(out.as_str()).width() > width - 1 {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}

pub(crate) fn aligned_row(
    width: usize,
    marker: &str,
    title: &str,
    data: &str,
    title_style: Style,
    theme: &Theme,
) -> ListItem<'static> {
    let marker = fit_row_text(marker, width);
    let remaining = width.saturating_sub(Line::raw(marker.as_str()).width());
    let right = fit_row_text(data, if remaining >= 20 { remaining / 2 } else { 0 });
    let right_width = Line::raw(right.as_str()).width();
    let left_width = remaining.saturating_sub(right_width + if right.is_empty() { 0 } else { 2 });
    let left = fit_row_text(title, left_width);
    let gap = remaining.saturating_sub(Line::raw(left.as_str()).width() + right_width);
    ListItem::new(Line::from(vec![
        Span::styled(marker, Style::default().fg(theme.accent)),
        Span::styled(left, title_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, Style::default().fg(theme.dim)),
    ]))
}
