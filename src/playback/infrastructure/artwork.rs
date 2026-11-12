use crate::playback::domain::artwork::Cell;
use base64::Engine;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_SIDE: u32 = 1024;

pub struct Cover {
    pub id: u64,
    pub png_base64: String,
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

pub fn decode(bytes: &[u8]) -> Option<Cover> {
    let image = image::load_from_memory(bytes).ok()?;
    let small = image.thumbnail(MAX_SIDE, MAX_SIDE).to_rgb8();
    let (width, height) = small.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let mut png = std::io::Cursor::new(Vec::new());
    small.write_to(&mut png, image::ImageFormat::Png).ok()?;
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    Some(Cover {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        png_base64: base64::engine::general_purpose::STANDARD.encode(png.into_inner()),
        width,
        height,
        rgb: small.into_raw(),
    })
}

impl Cover {
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
    fn pixel(&self, u: f32, v: f32) -> [u8; 3] {
        let x = ((u * self.width as f32) as u32).min(self.width - 1);
        let y = ((v * self.height as f32) as u32).min(self.height - 1);
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    pub fn cells(&self, cols: u16, rows: u16) -> Vec<Cell> {
        if cols < 8 || rows < 4 {
            return Vec::new();
        }
        let max_px_h = rows as f32 * 2.0;
        let aspect = self.width as f32 / self.height as f32;
        let px_h = max_px_h.min(cols as f32 / aspect);
        let px_w = px_h * aspect;
        let origin_x = (cols as f32 - px_w) / 2.0;
        let origin_y = (max_px_h - px_h) / 2.0;
        let mut cells = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let x = col as f32 + 0.5;
                let top_y = row as f32 * 2.0 + 0.5;
                let bottom_y = row as f32 * 2.0 + 1.5;
                let inside = |y: f32| {
                    x >= origin_x && x < origin_x + px_w && y >= origin_y && y < origin_y + px_h
                };
                let (top, bottom) = (inside(top_y), inside(bottom_y));
                if !top && !bottom {
                    continue;
                }
                let sample = |y: f32| self.pixel((x - origin_x) / px_w, (y - origin_y) / px_h);
                let upper = if top { sample(top_y) } else { sample(bottom_y) };
                let lower = if bottom {
                    sample(bottom_y)
                } else {
                    sample(top_y)
                };
                cells.push(Cell {
                    x: col,
                    y: row,
                    symbol: '▀',
                    depth: 0.5,
                    rgb: Some((upper, lower)),
                });
            }
        }
        cells
    }
}

pub fn download(url: &str) -> Option<Cover> {
    use std::io::Read;
    const LIMIT: u64 = 8 * 1024 * 1024;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client.get(url).send().ok()?.error_for_status().ok()?;
    let mut bytes = Vec::new();
    response.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 40, 40]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_and_scales() {
        let cover = decode(&png(400, 400)).unwrap();
        assert!(cover.width <= MAX_SIDE && cover.height <= MAX_SIDE);
        let cells = cover.cells(40, 12);
        assert!(!cells.is_empty());
        assert!(
            cells
                .iter()
                .all(|c| c.x < 40 && c.y < 12 && c.rgb.is_some())
        );
        let first = cells[0].rgb.unwrap();
        assert_eq!(first.0, [200, 40, 40]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"not an image").is_none());
        assert!(decode(&png(40, 40)).unwrap().cells(4, 2).is_empty());
    }
}
