use std::path::Path;

pub fn temp_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("crab-{label}-{}-{id}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return path.canonicalize().unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create test directory: {error}"),
        }
    }
}

pub fn write_wav(path: &Path, seconds: u32) {
    let rate = 44_100u32;
    let frames = rate * seconds;
    let data_len = frames * 4;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let value = ((std::f32::consts::TAU * 440.0 * t).sin() * 12_000.0) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}
