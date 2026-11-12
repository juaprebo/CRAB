use crate::settings::presentation::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use std::collections::VecDeque;

const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
const WAVE_GAIN: f32 = 1.6;
const WATERFALL_FLOOR: f32 = 0.05;

fn level_at(levels: &[f32], pos: f32) -> f32 {
    match levels.len() {
        0 => 0.0,
        1 => levels[0],
        len => {
            let scaled = pos.clamp(0.0, 1.0) * (len - 1) as f32;
            let low = scaled.floor() as usize;
            let high = (low + 1).min(len - 1);
            let t = scaled - low as f32;
            levels[low] * (1.0 - t) + levels[high] * t
        }
    }
}

fn position(index: usize, count: usize) -> f32 {
    if count > 1 {
        index as f32 / (count - 1) as f32
    } else {
        0.0
    }
}

struct Dots {
    cells_w: usize,
    cells_h: usize,
    bits: Vec<u8>,
}

impl Dots {
    fn new(area: Rect) -> Self {
        let cells_w = area.width as usize;
        let cells_h = area.height as usize;
        Self {
            cells_w,
            cells_h,
            bits: vec![0; cells_w * cells_h],
        }
    }

    fn width(&self) -> usize {
        self.cells_w * 2
    }

    fn height(&self) -> usize {
        self.cells_h * 4
    }

    fn set(&mut self, x: usize, y: usize) {
        if x >= self.width() || y >= self.height() {
            return;
        }
        self.bits[(y / 4) * self.cells_w + x / 2] |= BRAILLE_BITS[x % 2][y % 4];
    }

    fn vline(&mut self, x: usize, a: usize, b: usize) {
        for y in a.min(b)..=a.max(b) {
            self.set(x, y);
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer, color: impl Fn(usize) -> Color) {
        for row in 0..self.cells_h {
            let tint = color(row);
            for col in 0..self.cells_w {
                let bits = self.bits[row * self.cells_w + col];
                if bits == 0 {
                    continue;
                }
                let symbol = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                buf[(area.x + col as u16, area.y + row as u16)]
                    .set_char(symbol)
                    .set_fg(tint);
            }
        }
    }
}

pub struct MirrorView<'a> {
    pub levels: &'a [f32],
    pub theme: &'a Theme,
    pub bar_width: u16,
    pub gap: u16,
}

impl Widget for MirrorView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let half = area.height as usize / 2;
        if area.width == 0 || half == 0 || self.levels.is_empty() {
            return;
        }
        let top_pad = area.height as usize - half * 2;
        let step = self.bar_width + self.gap;
        let used = (self.levels.len() as u16 * step).saturating_sub(self.gap);
        let offset = area.width.saturating_sub(used) / 2;

        for (i, &level) in self.levels.iter().enumerate() {
            let x = area.x + offset + i as u16 * step;
            if x + self.bar_width > area.x + area.width {
                break;
            }
            let halves = (level.clamp(0.0, 1.0) * half as f32 * 2.0).round() as usize;
            for d in 0..half {
                let filled = halves.saturating_sub(d * 2).min(2);
                if filled == 0 {
                    break;
                }
                let color = self.theme.gradient((d as f32 + 0.5) / half as f32);
                let (up, down) = if filled == 2 {
                    ('█', '█')
                } else {
                    ('▄', '▀')
                };
                let up_y = area.y + (top_pad + half - 1 - d) as u16;
                let down_y = area.y + (top_pad + half + d) as u16;
                for dx in 0..self.bar_width {
                    buf[(x + dx, up_y)].set_char(up).set_fg(color);
                    buf[(x + dx, down_y)].set_char(down).set_fg(color);
                }
            }
        }
    }
}

pub struct CurveView<'a> {
    pub levels: &'a [f32],
    pub theme: &'a Theme,
    pub stipple: bool,
}

impl Widget for CurveView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.levels.is_empty() {
            return;
        }
        let mut dots = Dots::new(area);
        let (w, h) = (dots.width(), dots.height());
        let mut previous: Option<usize> = None;
        for x in 0..w {
            let level = level_at(self.levels, position(x, w)).clamp(0.0, 1.0);
            let y = ((1.0 - level) * (h - 1) as f32).round() as usize;
            if self.stipple {
                if level > 0.02 {
                    for yy in y..h {
                        if (x + yy) & 1 == 0 {
                            dots.set(x, yy);
                        }
                    }
                }
            } else {
                match previous {
                    Some(prev) => dots.vline(x, prev, y),
                    None => dots.set(x, y),
                }
                previous = Some(y);
            }
        }
        let rows = area.height as f32;
        dots.render(area, buf, |row| {
            self.theme.gradient(1.0 - (row as f32 + 0.5) / rows)
        });
    }
}

pub struct WaveView<'a> {
    pub wave: &'a [f32],
    pub theme: &'a Theme,
}

impl Widget for WaveView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut dots = Dots::new(area);
        let (w, h) = (dots.width(), dots.height());
        let count = self.wave.len();
        let center = (h - 1) as f32 / 2.0;
        let to_row = |value: f32| -> usize {
            let offset = (value * WAVE_GAIN).clamp(-1.0, 1.0) * center;
            (center - offset).round() as usize
        };
        for x in 0..w {
            if count == 0 {
                dots.set(x, to_row(0.0));
                continue;
            }
            let start = x * count / w;
            let end = ((x + 1) * count / w).clamp(start + 1, count);
            let (low, high) = self.wave[start..end]
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            dots.vline(x, to_row(high), to_row(low));
        }
        let rows = area.height as f32;
        dots.render(area, buf, |row| {
            let distance = ((row as f32 + 0.5) - rows / 2.0).abs() / (rows / 2.0);
            self.theme.gradient(distance.clamp(0.0, 1.0))
        });
    }
}

pub struct WaterfallView<'a> {
    pub history: &'a VecDeque<Vec<f32>>,
    pub theme: &'a Theme,
}

impl Widget for WaterfallView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = area.width as usize;
        let pixel_rows = area.height as usize * 2;
        let pixel = |row: usize, col: usize| -> Option<Color> {
            let line = self.history.get(row)?;
            let fade = 1.0 - 0.55 * row as f32 / pixel_rows as f32;
            let level = level_at(line, position(col, width)).powf(1.5) * fade;
            if level < WATERFALL_FLOOR {
                None
            } else {
                Some(self.theme.gradient(level.clamp(0.0, 1.0)))
            }
        };
        for cy in 0..area.height as usize {
            for cx in 0..width {
                let top = pixel(cy * 2, cx);
                let bottom = pixel(cy * 2 + 1, cx);
                let cell = &mut buf[(area.x + cx as u16, area.y + cy as u16)];
                match (top, bottom) {
                    (None, None) => {}
                    (Some(t), None) => {
                        cell.set_char('▀').set_fg(t);
                    }
                    (None, Some(b)) => {
                        cell.set_char('▄').set_fg(b);
                    }
                    (Some(t), Some(b)) => {
                        cell.set_char('▀').set_fg(t).set_bg(b);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::presentation::theme::builtin_themes;

    fn text(buf: &Buffer, area: Rect) -> Vec<String> {
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(area.x + x, area.y + y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    fn filled(rows: &[String]) -> usize {
        rows.iter()
            .flat_map(|r| r.chars())
            .filter(|c| *c != ' ')
            .count()
    }

    #[test]
    fn interpolates_levels() {
        let levels = [0.0, 1.0];
        assert_eq!(level_at(&levels, 0.0), 0.0);
        assert_eq!(level_at(&levels, 1.0), 1.0);
        assert!((level_at(&levels, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(level_at(&[], 0.5), 0.0);
        assert_eq!(level_at(&[0.7], 0.9), 0.7);
        assert_eq!(level_at(&levels, 4.0), 1.0);
    }

    #[test]
    fn braille_dots() {
        let area = Rect::new(0, 0, 2, 1);
        let mut dots = Dots::new(area);
        dots.set(0, 0);
        dots.set(1, 3);
        dots.set(9, 9);
        let mut buf = Buffer::empty(area);
        dots.render(area, &mut buf, |_| Color::White);
        assert_eq!(buf[(0, 0)].symbol(), "\u{2881}");
        assert_eq!(buf[(1, 0)].symbol(), " ");
        let mut tall = Dots::new(Rect::new(0, 0, 1, 2));
        tall.vline(0, 6, 1);
        assert_eq!(tall.bits, vec![0x02 | 0x04 | 0x40, 0x01 | 0x02 | 0x04]);
    }

    #[test]
    fn mirror_is_symmetric() {
        let themes = builtin_themes();
        let area = Rect::new(0, 0, 11, 8);
        let mut buf = Buffer::empty(area);
        MirrorView {
            levels: &[1.0, 0.5, 0.375, 0.0],
            theme: &themes[0],
            bar_width: 2,
            gap: 1,
        }
        .render(area, &mut buf);
        let rows = text(&buf, area);
        assert_eq!(rows[0], "██         ");
        assert_eq!(rows[1], "██         ");
        assert_eq!(rows[2], "██ ██ ▄▄   ");
        assert_eq!(rows[3], "██ ██ ██   ");
        assert_eq!(rows[4], rows[3]);
        assert_eq!(rows[5], "██ ██ ▀▀   ");
        assert_eq!(rows[6], rows[1]);
        assert_eq!(rows[7], rows[0]);
    }

    #[test]
    fn mirror_handles_degenerate_areas() {
        let themes = builtin_themes();
        for area in [
            Rect::new(0, 0, 0, 5),
            Rect::new(0, 0, 5, 1),
            Rect::new(0, 0, 5, 0),
        ] {
            let mut buf = Buffer::empty(Rect::new(0, 0, 6, 6));
            MirrorView {
                levels: &[1.0],
                theme: &themes[0],
                bar_width: 2,
                gap: 1,
            }
            .render(area, &mut buf);
        }
    }

    #[test]
    fn curve_line_and_stipple() {
        let themes = builtin_themes();
        let area = Rect::new(0, 0, 20, 6);
        let levels: Vec<f32> = (0..10).map(|i| i as f32 / 9.0).collect();

        let mut line = Buffer::empty(area);
        CurveView {
            levels: &levels,
            theme: &themes[0],
            stipple: false,
        }
        .render(area, &mut line);
        let line_rows = text(&line, area);
        assert!(!line_rows[5].starts_with(' '));
        assert!(!line_rows[0].ends_with(' '));
        assert!(line_rows[0].starts_with(' '));

        let mut stipple = Buffer::empty(area);
        CurveView {
            levels: &levels,
            theme: &themes[0],
            stipple: true,
        }
        .render(area, &mut stipple);
        assert!(filled(&text(&stipple, area)) > filled(&line_rows));

        let mut silent = Buffer::empty(area);
        CurveView {
            levels: &[0.0; 10],
            theme: &themes[0],
            stipple: true,
        }
        .render(area, &mut silent);
        assert_eq!(filled(&text(&silent, area)), 0);
    }

    #[test]
    fn wave_follows_signal() {
        let themes = builtin_themes();
        let area = Rect::new(0, 0, 24, 8);
        let wave: Vec<f32> = (0..1024)
            .map(|i| (std::f32::consts::TAU * i as f32 / 256.0).sin() * 0.6)
            .collect();
        let mut buf = Buffer::empty(area);
        WaveView {
            wave: &wave,
            theme: &themes[0],
        }
        .render(area, &mut buf);
        let rows = text(&buf, area);
        assert!(rows[0..3].iter().any(|r| r.contains(|c| c != ' ')));
        assert!(rows[5..8].iter().any(|r| r.contains(|c| c != ' ')));

        let mut flat = Buffer::empty(area);
        WaveView {
            wave: &[],
            theme: &themes[0],
        }
        .render(area, &mut flat);
        let flat_rows = text(&flat, area);
        let lit: Vec<usize> = flat_rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.contains(|c| c != ' '))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(lit.len(), 1);
        assert!(lit[0] == 3 || lit[0] == 4);
    }

    #[test]
    fn waterfall_draws_history() {
        let themes = builtin_themes();
        let area = Rect::new(0, 0, 12, 4);
        let mut history = VecDeque::new();
        history.push_back(vec![1.0; 8]);
        history.push_back(vec![0.0; 8]);
        history.push_back(vec![1.0; 8]);
        let mut buf = Buffer::empty(area);
        WaterfallView {
            history: &history,
            theme: &themes[0],
        }
        .render(area, &mut buf);
        let rows = text(&buf, area);
        assert_eq!(rows[0], "▀▀▀▀▀▀▀▀▀▀▀▀");
        assert_eq!(rows[1], "▀▀▀▀▀▀▀▀▀▀▀▀");
        assert!(rows[2].trim().is_empty());
        assert_ne!(buf[(0, 0)].fg, Color::Reset);
        assert_eq!(buf[(0, 0)].bg, Color::Reset);
    }
}
