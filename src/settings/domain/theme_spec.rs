use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CustomTheme {
    pub base: Option<String>,
    pub background: Option<String>,
    pub text: Option<String>,
    pub dim: Option<String>,
    pub border: Option<String>,
    pub accent: Option<String>,
    pub secondary: Option<String>,
    pub on_accent: Option<String>,
    pub error: Option<String>,
    pub ok: Option<String>,
    pub bars: Option<Vec<String>>,
    pub art: Option<String>,
}
