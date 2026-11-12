use crate::playlists::domain::playlist::{self, Playlist};
use crate::playlists::domain::playlist_repository::PlaylistRepository;
use anyhow::Result;
use std::path::PathBuf;

pub struct PlaylistService {
    repository: Box<dyn PlaylistRepository>,
}

impl PlaylistService {
    pub fn new(repository: Box<dyn PlaylistRepository>) -> Self {
        Self { repository }
    }
    pub fn load(&self) -> Vec<Playlist> {
        self.repository.load()
    }
    pub fn save(&self, playlists: &[Playlist]) -> Result<()> {
        self.repository.save(playlists)
    }

    pub fn rename(
        &self,
        playlists: &mut Vec<Playlist>,
        index: usize,
        name: &str,
    ) -> Result<String> {
        let mut updated = playlists.clone();
        let previous = playlist::rename(&mut updated, index, name)?;
        self.repository.save(&updated)?;
        *playlists = updated;
        Ok(previous)
    }

    pub fn add_tracks(
        &self,
        playlists: &mut Vec<Playlist>,
        name: &str,
        paths: Vec<PathBuf>,
    ) -> Result<(usize, usize)> {
        let mut updated = playlists.clone();
        let result = playlist::add_tracks(&mut updated, name, paths);
        self.repository.save(&updated)?;
        *playlists = updated;
        Ok(result)
    }

    pub fn remove(&self, playlists: &mut Vec<Playlist>, index: usize) -> Result<()> {
        let mut updated = playlists.clone();
        anyhow::ensure!(index < updated.len(), "Playlist no longer exists");
        updated.remove(index);
        self.repository.save(&updated)?;
        *playlists = updated;
        Ok(())
    }

    pub fn remove_tracks(
        &self,
        playlists: &mut Vec<Playlist>,
        list: usize,
        indices: &[usize],
    ) -> Result<()> {
        let mut updated = playlists.clone();
        let playlist = updated
            .get_mut(list)
            .ok_or(playlist::PlaylistError::MissingPlaylist)?;
        let mut index = 0;
        playlist.tracks.retain(|_| {
            let keep = !indices.contains(&index);
            index += 1;
            keep
        });
        self.repository.save(&updated)?;
        *playlists = updated;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    struct MemoryRepository {
        saved: Rc<RefCell<Vec<Playlist>>>,
        fail: Rc<Cell<bool>>,
    }
    impl PlaylistRepository for MemoryRepository {
        fn load(&self) -> Vec<Playlist> {
            self.saved.borrow().clone()
        }
        fn save(&self, playlists: &[Playlist]) -> Result<()> {
            anyhow::ensure!(!self.fail.get(), "storage unavailable");
            *self.saved.borrow_mut() = playlists.to_vec();
            Ok(())
        }
    }

    #[test]
    fn mutations_commit_together_and_failed_writes_preserve_the_catalog() {
        let saved = Rc::new(RefCell::new(Vec::new()));
        let fail = Rc::new(Cell::new(false));
        let service = PlaylistService::new(Box::new(MemoryRepository {
            saved: saved.clone(),
            fail: fail.clone(),
        }));
        let mut lists = service.load();
        let path = PathBuf::from("/music/a.mp3");
        assert_eq!(
            service
                .add_tracks(&mut lists, "One", vec![path.clone(), path.clone()])
                .unwrap(),
            (0, 1)
        );
        service.add_tracks(&mut lists, "Two", vec![]).unwrap();
        let original = lists.clone();
        assert!(service.rename(&mut lists, 0, "two").is_err());
        assert_eq!(lists, original);
        fail.set(true);
        assert!(service.rename(&mut lists, 0, "Changed").is_err());
        assert!(service.add_tracks(&mut lists, "New", vec![path]).is_err());
        assert!(service.remove(&mut lists, 0).is_err());
        assert!(service.remove_tracks(&mut lists, 0, &[0]).is_err());
        assert_eq!(lists, original);
        assert_eq!(*saved.borrow(), original);
        fail.set(false);
        service.rename(&mut lists, 0, "Changed").unwrap();
        service.remove_tracks(&mut lists, 0, &[0]).unwrap();
        assert!(lists[0].tracks.is_empty());
        service.remove(&mut lists, 1).unwrap();
        assert_eq!(*saved.borrow(), lists);
        assert_eq!(lists[0].name, "Changed");
    }
}
