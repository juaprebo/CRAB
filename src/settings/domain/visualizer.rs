use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VizStyle {
    #[default]
    Bars,
    Mirror,
    Line,
    Dots,
    Wave,
    Waterfall,
}

impl VizStyle {
    pub const ALL: [VizStyle; 6] = [
        VizStyle::Bars,
        VizStyle::Mirror,
        VizStyle::Line,
        VizStyle::Dots,
        VizStyle::Wave,
        VizStyle::Waterfall,
    ];

    pub fn from_name(name: &str) -> Option<VizStyle> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }

    pub fn name(self) -> &'static str {
        match self {
            VizStyle::Bars => "bars",
            VizStyle::Mirror => "mirror",
            VizStyle::Line => "line",
            VizStyle::Dots => "dots",
            VizStyle::Wave => "wave",
            VizStyle::Waterfall => "waterfall",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            VizStyle::Bars => "Spectrum",
            VizStyle::Mirror => "Spectrum - mirror",
            VizStyle::Line => "Spectrum - line",
            VizStyle::Dots => "Spectrum - dots",
            VizStyle::Wave => "Waveform",
            VizStyle::Waterfall => "Spectrogram",
        }
    }

    pub fn next(self) -> VizStyle {
        let index = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}
