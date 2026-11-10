const ASPECT: f32 = 1.3;

#[derive(Clone, Debug)]
pub enum Art {
    Crab,
    Text(Vec<String>),
}

pub struct Cell {
    pub x: u16,
    pub y: u16,
    pub symbol: char,
    pub depth: f32,
}

fn segment_distance(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let length_sq = vx * vx + vy * vy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        ((wx * vx + wy * vy) / length_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + vx * t, ay + vy * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

fn circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> bool {
    (px - cx).powi(2) + (py - cy).powi(2) <= r * r
}

fn polyline(px: f32, py: f32, points: &[(f32, f32)], r: f32) -> bool {
    points
        .windows(2)
        .any(|w| segment_distance(px, py, w[0].0, w[0].1, w[1].0, w[1].1) <= r)
}

fn claw(dx: f32, y: f32) -> bool {
    let (cx, cy, r) = (0.5_f32, 0.23_f32, 0.15_f32);
    if !circle(dx, y, cx, cy, r) {
        return false;
    }
    let (ux, uy) = (-0.55_f32, -0.83_f32);
    let norm = (ux * ux + uy * uy).sqrt();
    let (ux, uy) = (ux / norm, uy / norm);
    let (vx, vy) = (dx - cx, y - cy);
    let along = vx * ux + vy * uy;
    if along <= -0.01 {
        return true;
    }
    let across = (vx * -uy + vy * ux).abs();
    across > along * 0.6
}

fn inside(dx: f32, y: f32) -> bool {
    let body = (dx / 0.36).powi(2) + ((y - 0.63) / 0.235).powi(2) <= 1.0;
    let eyes =
        polyline(dx, y, &[(0.10, 0.47), (0.11, 0.38)], 0.03) || circle(dx, y, 0.11, 0.36, 0.04);
    let arms = polyline(dx, y, &[(0.31, 0.58), (0.46, 0.46), (0.50, 0.32)], 0.028);
    let legs = polyline(dx, y, &[(0.30, 0.72), (0.46, 0.74), (0.54, 0.86)], 0.022)
        || polyline(dx, y, &[(0.27, 0.78), (0.48, 0.84), (0.56, 0.96)], 0.022)
        || polyline(dx, y, &[(0.22, 0.83), (0.43, 0.92), (0.48, 0.99)], 0.022);
    if !(body || eyes || arms || legs || claw(dx, y)) {
        return false;
    }
    let holes = circle(dx, y, 0.0, 0.60, 0.022)
        || circle(dx, y, 0.17, 0.66, 0.018)
        || circle(dx, y, 0.21, 0.56, 0.015)
        || circle(dx, y, 0.08, 0.72, 0.016)
        || circle(dx, y, 0.11, 0.36, 0.014);
    !holes
}

pub fn crab_cells(cols: u16, rows: u16) -> Vec<Cell> {
    if cols < 8 || rows < 4 {
        return Vec::new();
    }
    let max_height = rows as f32 * 2.0;
    let height = max_height.min(cols as f32 / ASPECT);
    let width = height * ASPECT;
    let origin_x = (cols as f32 - width) / 2.0;
    let origin_y = (max_height - height) / 2.0;
    let mut cells = Vec::new();

    let sample = |px: f32, py: f32| -> bool {
        let u = (px - origin_x) / width;
        let v = (py - origin_y) / height;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return false;
        }
        let dx = (u - 0.5).abs() * ASPECT;
        inside(dx, v)
    };

    for row in 0..rows {
        for col in 0..cols {
            let x = col as f32 + 0.5;
            let top = sample(x, row as f32 * 2.0 + 0.5);
            let bottom = sample(x, row as f32 * 2.0 + 1.5);
            let symbol = match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => continue,
            };
            let v = ((row as f32 * 2.0 + 1.0) - origin_y) / height;
            cells.push(Cell {
                x: col,
                y: row,
                symbol,
                depth: v.clamp(0.0, 1.0),
            });
        }
    }
    cells
}

pub fn text_cells(lines: &[String], cols: u16, rows: u16) -> Vec<Cell> {
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    if width == 0 || width as u16 > cols || lines.len() as u16 > rows {
        return Vec::new();
    }
    let origin_x = (cols as usize - width) / 2;
    let origin_y = (rows as usize - lines.len()) / 2;
    let total = lines.len().max(2) as f32 - 1.0;
    let mut cells = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        for (col, symbol) in line.chars().enumerate() {
            if symbol == ' ' {
                continue;
            }
            cells.push(Cell {
                x: (origin_x + col) as u16,
                y: (origin_y + row) as u16,
                symbol,
                depth: row as f32 / total,
            });
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crab_symmetric() {
        let cells = crab_cells(60, 18);
        assert!(!cells.is_empty());
        assert!(cells.iter().all(|c| c.x < 60 && c.y < 18));
        let left = cells.iter().filter(|c| c.x < 30).count();
        let right = cells.iter().filter(|c| c.x >= 30).count();
        assert!(left.abs_diff(right) <= 18);
    }

    #[test]
    fn tiny_area() {
        assert!(crab_cells(5, 3).is_empty());
    }
}
