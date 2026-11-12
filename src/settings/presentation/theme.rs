use crate::settings::domain::theme_spec::CustomTheme;
use crate::settings::infrastructure::config::toml_key;
use ratatui::style::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub background: Option<Color>,
    pub text: Color,
    pub dim: Color,
    pub border: Color,
    pub accent: Color,
    pub secondary: Color,
    pub on_accent: Color,
    pub error: Color,
    pub ok: Color,
    pub bars: Vec<(u8, u8, u8)>,
    pub art: Option<Color>,
}

pub fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn color(value: &str) -> Color {
    let (r, g, b) = parse_hex(value).unwrap_or((255, 0, 255));
    Color::Rgb(r, g, b)
}

pub fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    }
}

pub fn to_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

pub fn to_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn to_hsl((r, g, b): (u8, u8, u8)) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h * 60.0, s.clamp(0.0, 1.0), l)
}

pub fn from_hsl(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(r), byte(g), byte(b))
}

pub fn generate(accent: (u8, u8, u8), dark: bool) -> Theme {
    let (h, s, l) = to_hsl(accent);
    let s = s.max(0.35);
    let tint = |hue: f32, sat: f32, light: f32| to_color(from_hsl(hue, sat, light));
    let background = if dark {
        from_hsl(h, 0.3, 0.09)
    } else {
        from_hsl(h, 0.35, 0.95)
    };
    let luma = 0.2126 * accent.0 as f32 + 0.7152 * accent.1 as f32 + 0.0722 * accent.2 as f32;
    let on_accent = if luma > 127.0 {
        from_hsl(h, 0.3, 0.08)
    } else {
        from_hsl(h, 0.2, 0.97)
    };
    let (error, ok) = if dark {
        ((255, 110, 110), (120, 220, 140))
    } else {
        ((192, 57, 43), (39, 130, 80))
    };
    Theme {
        name: "generated".to_string(),
        background: Some(to_color(background)),
        text: if dark {
            tint(h, 0.2, 0.92)
        } else {
            tint(h, 0.3, 0.14)
        },
        dim: if dark {
            tint(h, 0.15, 0.55)
        } else {
            tint(h, 0.15, 0.45)
        },
        border: if dark {
            tint(h, 0.2, 0.26)
        } else {
            tint(h, 0.2, 0.8)
        },
        accent: to_color(accent),
        secondary: tint(h + 35.0, s, l.clamp(0.45, 0.7)),
        on_accent: to_color(on_accent),
        error: to_color(error),
        ok: to_color(ok),
        bars: vec![
            from_hsl(h - 30.0, s, (l * 0.8).clamp(0.3, 0.6)),
            accent,
            from_hsl(h + 40.0, s, (l + 0.2).clamp(0.55, 0.85)),
        ],
        art: None,
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

#[allow(clippy::too_many_arguments)]
fn builtin(
    name: &str,
    background: Option<&str>,
    text: &str,
    dim: &str,
    border: &str,
    accent: &str,
    secondary: &str,
    on_accent: &str,
    bars: &[&str],
) -> Theme {
    Theme {
        name: name.to_string(),
        background: background.map(color),
        text: color(text),
        dim: color(dim),
        border: color(border),
        accent: color(accent),
        secondary: color(secondary),
        on_accent: color(on_accent),
        error: color("#ff6e6e"),
        ok: color("#78dc8c"),
        bars: bars.iter().filter_map(|b| parse_hex(b)).collect(),
        art: None,
    }
}

pub fn builtin_themes() -> Vec<Theme> {
    vec![
        builtin(
            "crab",
            None,
            "#e4e4ee",
            "#7a7a8c",
            "#424258",
            "#ff5a4e",
            "#ff9f43",
            "#000000",
            &["#ff5a4e", "#ff9f43", "#ffe896"],
        ),
        builtin(
            "synthwave",
            Some("#140b2e"),
            "#f0e6ff",
            "#8a77b8",
            "#3a2f5c",
            "#ff2fa8",
            "#b967ff",
            "#140b2e",
            &["#00e5ff", "#b967ff", "#ff2fa8"],
        ),
        builtin(
            "ocean",
            None,
            "#e2f1f8",
            "#6f8fa0",
            "#2f4858",
            "#2dd4bf",
            "#38bdf8",
            "#04202a",
            &["#0ea5e9", "#2dd4bf", "#a7f3d0"],
        ),
        builtin(
            "matrix",
            None,
            "#d9f99d",
            "#5b7f4a",
            "#25402a",
            "#4ade80",
            "#a3e635",
            "#04140a",
            &["#166534", "#4ade80", "#d9f99d"],
        ),
        builtin(
            "nord",
            Some("#2e3440"),
            "#eceff4",
            "#7b88a1",
            "#434c5e",
            "#88c0d0",
            "#81a1c1",
            "#2e3440",
            &["#5e81ac", "#88c0d0", "#a3be8c", "#ebcb8b"],
        ),
        builtin(
            "dracula",
            Some("#282a36"),
            "#f8f8f2",
            "#6272a4",
            "#44475a",
            "#bd93f9",
            "#ff79c6",
            "#282a36",
            &["#6272a4", "#bd93f9", "#ff79c6", "#f1fa8c"],
        ),
        builtin(
            "gruvbox",
            Some("#282828"),
            "#ebdbb2",
            "#928374",
            "#504945",
            "#fe8019",
            "#fabd2f",
            "#282828",
            &["#cc241d", "#fe8019", "#fabd2f", "#b8bb26"],
        ),
        builtin(
            "mono",
            None,
            "#e5e5e5",
            "#777777",
            "#444444",
            "#e5e5e5",
            "#b0b0b0",
            "#000000",
            &["#555555", "#aaaaaa", "#ffffff"],
        ),
        builtin(
            "paper",
            Some("#faf4e8"),
            "#3b3228",
            "#8c7f70",
            "#d6c9b4",
            "#d2491f",
            "#e08a1e",
            "#ffffff",
            &["#d2491f", "#e08a1e", "#c9a227"],
        ),
        builtin(
            "rustbox",
            Some("#e8e0c8"),
            "#1d2833",
            "#8a8470",
            "#c9bfa0",
            "#b5361b",
            "#3d6670",
            "#f4eedb",
            &["#3d6670", "#b5361b", "#d9532a"],
        ),
    ]
}

pub fn resolve(name: &str, custom: &CustomTheme, all: &[Theme]) -> Theme {
    let base_name = custom.base.as_deref().unwrap_or("crab");
    let mut theme = all
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(base_name))
        .or_else(|| all.first())
        .cloned()
        .expect("at least one builtin theme");
    theme.name = name.to_string();
    let set = |slot: &mut Color, value: &Option<String>| {
        if let Some(value) = value
            && parse_hex(value).is_some()
        {
            *slot = color(value);
        }
    };
    if let Some(value) = &custom.background {
        theme.background = parse_hex(value).map(|(r, g, b)| Color::Rgb(r, g, b));
    }
    set(&mut theme.text, &custom.text);
    set(&mut theme.dim, &custom.dim);
    set(&mut theme.border, &custom.border);
    set(&mut theme.accent, &custom.accent);
    set(&mut theme.secondary, &custom.secondary);
    set(&mut theme.on_accent, &custom.on_accent);
    set(&mut theme.error, &custom.error);
    set(&mut theme.ok, &custom.ok);
    if let Some(bars) = &custom.bars {
        let parsed: Vec<_> = bars.iter().filter_map(|b| parse_hex(b)).collect();
        if parsed.len() >= 2 {
            theme.bars = parsed;
        } else if parsed.len() == 1 {
            theme.bars = vec![parsed[0], parsed[0]];
        }
    }
    if let Some(value) = &custom.art {
        theme.art = parse_hex(value).map(|(r, g, b)| Color::Rgb(r, g, b));
    }
    theme
}

impl Theme {
    pub fn to_toml_section(&self, name: &str) -> String {
        let mut out = format!("[themes.{}]\n", toml_key(name));
        if let Some(background) = self.background {
            out.push_str(&format!("background = \"{}\"\n", to_hex(rgb(background))));
        }
        let fields = [
            ("text", self.text),
            ("dim", self.dim),
            ("border", self.border),
            ("accent", self.accent),
            ("secondary", self.secondary),
            ("on_accent", self.on_accent),
            ("error", self.error),
            ("ok", self.ok),
        ];
        for (key, value) in fields {
            out.push_str(&format!("{key} = \"{}\"\n", to_hex(rgb(value))));
        }
        if let Some(art) = self.art {
            out.push_str(&format!("art = \"{}\"\n", to_hex(rgb(art))));
        }
        let bars: Vec<String> = self
            .bars
            .iter()
            .map(|bar| format!("\"{}\"", to_hex(*bar)))
            .collect();
        out.push_str(&format!("bars = [{}]\n", bars.join(", ")));
        out
    }

    pub fn gradient(&self, t: f32) -> Color {
        let (r, g, b) = self.gradient_rgb(t);
        Color::Rgb(r, g, b)
    }

    fn gradient_rgb(&self, t: f32) -> (u8, u8, u8) {
        let t = t.clamp(0.0, 1.0);
        let segments = self.bars.len().saturating_sub(1).max(1);
        let scaled = t * segments as f32;
        let index = (scaled.floor() as usize).min(segments - 1);
        let local = scaled - index as f32;
        let from = self.bars.get(index).copied().unwrap_or((255, 255, 255));
        let to = self.bars.get(index + 1).copied().unwrap_or(from);
        mix(from, to, local)
    }

    pub fn art_on_accent(&self, strength: f32) -> Color {
        let (r, g, b) = mix(
            rgb(self.accent),
            rgb(self.on_accent),
            (strength * 0.7).clamp(0.08, 0.6),
        );
        Color::Rgb(r, g, b)
    }

    pub fn art_color(&self, t: f32, strength: f32) -> Color {
        if let Some(color) = self.art {
            return color;
        }
        let background = self.background.map(rgb).unwrap_or((16, 16, 20));
        let (r, g, b) = mix(background, self.gradient_rgb(t), strength);
        Color::Rgb(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex("#ff5a4e"), Some((255, 90, 78)));
        assert_eq!(parse_hex("00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("#gggggg"), None);
    }

    #[test]
    fn inherits_base() {
        let all = builtin_themes();
        let custom = CustomTheme {
            base: Some("nord".to_string()),
            accent: Some("#ff0000".to_string()),
            ..Default::default()
        };
        let theme = resolve("mine", &custom, &all);
        assert_eq!(theme.name, "mine");
        assert_eq!(theme.accent, Color::Rgb(255, 0, 0));
        assert_eq!(theme.background, Some(Color::Rgb(0x2e, 0x34, 0x40)));
    }

    #[test]
    fn section_roundtrip() {
        let all = builtin_themes();
        for original in &all {
            let section = original.to_toml_section("copy");
            let parsed: std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<String, CustomTheme>,
            > = toml::from_str(&section).unwrap();
            let custom = &parsed["themes"]["copy"];
            let rebuilt = resolve("copy", custom, &all);
            assert_eq!(rebuilt.background, original.background);
            assert_eq!(rebuilt.text, original.text);
            assert_eq!(rebuilt.accent, original.accent);
            assert_eq!(rebuilt.on_accent, original.on_accent);
            assert_eq!(rebuilt.bars, original.bars);
        }
    }

    #[test]
    fn hsl_roundtrip() {
        for rgb in [
            (255, 90, 78),
            (0, 229, 255),
            (20, 11, 46),
            (128, 128, 128),
            (255, 255, 255),
        ] {
            let (h, s, l) = to_hsl(rgb);
            let back = from_hsl(h, s, l);
            assert!(
                back.0.abs_diff(rgb.0) <= 1
                    && back.1.abs_diff(rgb.1) <= 1
                    && back.2.abs_diff(rgb.2) <= 1
            );
        }
        assert_eq!(from_hsl(0.0, 1.0, 0.5), (255, 0, 0));
        assert_eq!(from_hsl(120.0, 1.0, 0.5), (0, 255, 0));
    }

    #[test]
    fn generated_themes_are_readable() {
        let luminance = |c: Color| {
            let (r, g, b) = rgb(c);
            0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
        };
        for accent in [
            (255, 90, 78),
            (45, 212, 191),
            (185, 103, 255),
            (250, 204, 21),
        ] {
            let dark = generate(accent, true);
            let light = generate(accent, false);
            assert_eq!(dark.accent, to_color(accent));
            assert!(luminance(dark.text) - luminance(dark.background.unwrap()) > 120.0);
            assert!(luminance(light.background.unwrap()) - luminance(light.text) > 120.0);
            let contrast = (luminance(dark.on_accent) - luminance(dark.accent)).abs();
            assert!(contrast > 60.0);
            assert!(dark.bars.len() >= 2);
        }
    }

    #[test]
    fn key_quoting() {
        assert_eq!(toml_key("my-theme_2"), "my-theme_2");
        assert_eq!(toml_key("my theme"), "\"my theme\"");
    }

    #[test]
    fn art_on_accent_sits_between_bar_and_text() {
        for theme in builtin_themes() {
            let faint = rgb(theme.art_on_accent(0.1));
            let strong = rgb(theme.art_on_accent(1.0));
            let (accent, on_accent) = (rgb(theme.accent), rgb(theme.on_accent));
            let dist = |a: (u8, u8, u8), b: (u8, u8, u8)| {
                (a.0 as i32 - b.0 as i32).abs()
                    + (a.1 as i32 - b.1 as i32).abs()
                    + (a.2 as i32 - b.2 as i32).abs()
            };
            assert!(dist(faint, accent) <= dist(strong, accent));
            assert!(dist(strong, accent) < dist(on_accent, accent) || accent == on_accent);
        }
    }

    #[test]
    fn gradient_ends() {
        let theme = &builtin_themes()[0];
        assert_eq!(theme.gradient(0.0), Color::Rgb(255, 90, 78));
        assert_eq!(theme.gradient(1.0), Color::Rgb(255, 232, 150));
    }
}
