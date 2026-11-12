//! Native Kitty images; the UI renders half blocks on other terminals.
use crate::playback::infrastructure::artwork::Cover;
use ratatui::layout::Rect;
use std::io::{self, Write};

pub struct Display {
    pub enabled: bool,
    image_id: u32,
    loaded: Option<u64>,
    placed: Option<Rect>,
}

pub fn supports_kitty(
    term: &str,
    program: &str,
    kitty_window: bool,
    multiplexed: bool,
    blocks: bool,
) -> bool {
    !blocks && !multiplexed && (term == "xterm-kitty" || program == "kitty" || kitty_window)
}

impl Display {
    pub fn new() -> Self {
        Self {
            enabled: supports_kitty(
                &std::env::var("TERM").unwrap_or_default(),
                &std::env::var("TERM_PROGRAM").unwrap_or_default(),
                std::env::var_os("KITTY_WINDOW_ID").is_some(),
                std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some(),
                std::env::var("CRAB_COVER_MODE").is_ok_and(|v| v == "blocks"),
            ),
            image_id: 0x4300_0000 | (std::process::id() & 0x00ff_ffff),
            loaded: None,
            placed: None,
        }
    }

    pub fn render(
        &mut self,
        out: &mut impl Write,
        cover: Option<&Cover>,
        area: Option<Rect>,
        cell_pixels: (u16, u16),
    ) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let target = cover
            .zip(area)
            .map(|(cover, area)| (cover, fit(area, cover, cell_pixels)))
            .filter(|(_, rect)| rect.width > 0 && rect.height > 0);
        let Some((cover, rect)) = target else {
            if self.placed.take().is_some() {
                write!(out, "\x1b_Ga=d,d=i,i={},q=2;\x1b\\", self.image_id)?;
                out.flush()?;
            }
            return Ok(());
        };
        if self.loaded == Some(cover.id) && self.placed == Some(rect) {
            return Ok(());
        }
        // Save the cursor so graphics never disturb ratatui's bookkeeping.
        write!(out, "\x1b7")?;
        if self.loaded != Some(cover.id) {
            if self.loaded.is_some() {
                write!(out, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", self.image_id)?;
            }
            upload(out, self.image_id, &cover.png_base64)?;
            self.loaded = Some(cover.id);
        } else if self.placed.is_some() {
            write!(out, "\x1b_Ga=d,d=i,i={},q=2;\x1b\\", self.image_id)?;
        }
        write!(
            out,
            "\x1b[{};{}H\x1b_Ga=p,i={},c={},r={},C=1,z=1,q=2;\x1b\\\x1b8",
            rect.y + 1,
            rect.x + 1,
            self.image_id,
            rect.width,
            rect.height
        )?;
        self.placed = Some(rect);
        out.flush()
    }

    pub fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.enabled && self.loaded.take().is_some() {
            write!(out, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", self.image_id)?;
            self.placed = None;
            out.flush()?;
        }
        Ok(())
    }
}

fn upload(out: &mut impl Write, id: u32, data: &str) -> io::Result<()> {
    let chunks = data.as_bytes().chunks(4096);
    let count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        if index == 0 {
            write!(out, "\x1b_Ga=t,t=d,f=100,i={id},q=2,")?;
        } else {
            write!(out, "\x1b_G")?;
        }
        write!(out, "m={};", usize::from(index + 1 < count))?;
        out.write_all(chunk)?;
        write!(out, "\x1b\\")?;
    }
    Ok(())
}

fn fit(area: Rect, cover: &Cover, cell_pixels: (u16, u16)) -> Rect {
    let (cw, ch) = (cell_pixels.0.max(1) as f32, cell_pixels.1.max(1) as f32);
    let aspect = cover.aspect();
    let height = (area.height as f32).min(area.width as f32 * cw / ch / aspect);
    let width = (height * ch * aspect / cw).round() as u16;
    let height = height.round() as u16;
    let (width, height) = (width.min(area.width), height.min(area.height));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover() -> Cover {
        let image = image::RgbImage::from_pixel(100, 100, image::Rgb([230, 80, 50]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        crate::playback::infrastructure::artwork::decode(&bytes.into_inner()).unwrap()
    }

    #[test]
    fn detection_preserves_portable_and_multiplexer_fallbacks() {
        assert!(supports_kitty("xterm-kitty", "", false, false, false));
        assert!(supports_kitty(
            "xterm-256color",
            "kitty",
            false,
            false,
            false
        ));
        assert!(!supports_kitty("xterm-256color", "", false, false, false));
        assert!(!supports_kitty("xterm-kitty", "", true, true, false));
        assert!(!supports_kitty("xterm-kitty", "kitty", true, false, true));
    }

    #[test]
    fn uploads_are_chunked_and_reassembled_without_data_loss() {
        let data = "AAAA".repeat(2400);
        let mut out = Vec::new();
        upload(&mut out, 123, &data).unwrap();
        let text = String::from_utf8(out).unwrap();
        let commands: Vec<_> = text
            .split("\x1b_G")
            .skip(1)
            .map(|chunk| chunk.strip_suffix("\x1b\\").unwrap())
            .collect();
        assert_eq!(commands.len(), 3);
        assert!(commands[0].starts_with("a=t,t=d,f=100,i=123,q=2,m=1;"));
        assert!(commands.last().unwrap().starts_with("m=0;"));
        let chunks: Vec<_> = commands
            .iter()
            .map(|chunk| chunk.split_once(';').unwrap().1)
            .collect();
        assert!(chunks.iter().all(|chunk| chunk.len() <= 4096));
        assert_eq!(chunks.concat(), data);
    }

    #[test]
    fn image_lifecycle_caches_uploads_and_clears_only_its_own_image() {
        let cover = cover();
        let mut display = Display {
            enabled: true,
            image_id: 123,
            loaded: None,
            placed: None,
        };
        let mut out = Vec::new();
        let area = Rect::new(5, 4, 30, 10);
        display
            .render(&mut out, Some(&cover), Some(area), (8, 16))
            .unwrap();
        let first = String::from_utf8(out.clone()).unwrap();
        assert!(first.contains("a=t,"));
        assert!(first.contains("a=p,i=123,"));
        assert!(first.ends_with("\x1b8"));
        out.clear();
        display
            .render(&mut out, Some(&cover), Some(area), (8, 16))
            .unwrap();
        assert!(out.is_empty());
        display
            .render(&mut out, Some(&cover), None, (8, 16))
            .unwrap();
        assert!(
            String::from_utf8(out.clone())
                .unwrap()
                .contains("a=d,d=i,i=123,")
        );
        out.clear();
        display
            .render(
                &mut out,
                Some(&cover),
                Some(Rect::new(10, 3, 20, 8)),
                (8, 16),
            )
            .unwrap();
        assert!(!String::from_utf8(out.clone()).unwrap().contains("a=t,"));
        out.clear();
        display.clear(&mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("a=d,d=I,i=123,"));
    }

    #[test]
    fn images_fit_physical_cell_dimensions_without_stretching() {
        let cover = cover();
        assert_eq!(
            fit(Rect::new(0, 0, 30, 10), &cover, (8, 16)),
            Rect::new(5, 0, 20, 10)
        );
        let mut display = Display {
            enabled: false,
            image_id: 123,
            loaded: None,
            placed: None,
        };
        let mut out = Vec::new();
        display
            .render(
                &mut out,
                Some(&cover),
                Some(Rect::new(0, 0, 30, 10)),
                (8, 16),
            )
            .unwrap();
        assert!(out.is_empty());
    }
}
