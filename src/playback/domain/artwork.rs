pub struct Cell {
    pub x: u16,
    pub y: u16,
    pub symbol: char,
    pub depth: f32,
    pub rgb: Option<([u8; 3], [u8; 3])>,
}
