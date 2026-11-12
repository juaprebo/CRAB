use super::playlist::Playlist;
use anyhow::Result;

pub trait PlaylistRepository {
    fn load(&self) -> Vec<Playlist>;
    fn save(&self, playlists: &[Playlist]) -> Result<()>;
}
