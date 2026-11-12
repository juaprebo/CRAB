use crate::radio::domain::{
    station::Station,
    station_repository::{StationRepository, validate_removal},
};
use anyhow::Result;

pub struct StationService {
    repository: Box<dyn StationRepository>,
}
impl StationService {
    pub fn new(repository: Box<dyn StationRepository>) -> Self {
        Self { repository }
    }
    pub fn load(&self) -> Result<Vec<Station>> {
        self.repository.load()
    }
    pub fn can_remove(&self, stations: &[Station], index: usize) -> Result<()> {
        Ok(validate_removal(stations, index)?)
    }
    pub fn add(&self, stations: &mut Vec<Station>, station: Station) -> Result<usize> {
        if let Some(index) = stations
            .iter()
            .position(|existing| existing.url == station.url)
        {
            return Ok(index);
        }
        let mut updated = stations.clone();
        updated.push(station);
        self.repository.save(&updated)?;
        *stations = updated;
        Ok(stations.len() - 1)
    }
    pub fn remove(&self, stations: &mut Vec<Station>, index: usize) -> Result<Station> {
        validate_removal(stations, index)?;
        let mut updated = stations.clone();
        let removed = updated.remove(index);
        self.repository.save(&updated)?;
        *stations = updated;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct Repository {
        fail: Rc<Cell<bool>>,
        saved: Rc<RefCell<Vec<Station>>>,
    }
    impl StationRepository for Repository {
        fn load(&self) -> Result<Vec<Station>> {
            Ok(self.saved.borrow().clone())
        }
        fn save(&self, stations: &[Station]) -> Result<()> {
            anyhow::ensure!(!self.fail.get(), "station storage unavailable");
            *self.saved.borrow_mut() = stations.to_vec();
            Ok(())
        }
    }
    fn station(name: &str) -> Station {
        Station {
            name: name.into(),
            url: format!("https://example.com/{name}"),
        }
    }
    #[test]
    fn deduplicates_stations_preserves_the_last_station_and_rolls_back_failed_writes() {
        let fail = Rc::new(Cell::new(false));
        let saved = Rc::new(RefCell::new(vec![station("One")]));
        let service = StationService::new(Box::new(Repository {
            fail: fail.clone(),
            saved: saved.clone(),
        }));
        let mut stations = service.load().unwrap();
        assert!(service.remove(&mut stations, 0).is_err());
        assert_eq!(service.add(&mut stations, station("One")).unwrap(), 0);
        service.add(&mut stations, station("Two")).unwrap();
        let original = stations.clone();
        fail.set(true);
        assert!(service.remove(&mut stations, 0).is_err());
        assert!(service.add(&mut stations, station("Three")).is_err());
        assert_eq!(stations, original);
        assert_eq!(*saved.borrow(), original);
        fail.set(false);
        assert_eq!(service.remove(&mut stations, 0).unwrap().name, "One");
        assert_eq!(stations.len(), 1);
    }
}
