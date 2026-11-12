use crate::radio::domain::station_search_repository::{Found, StationSearchRepository};
use std::sync::{
    Arc,
    mpsc::{Receiver, channel},
};

#[derive(Clone)]
pub struct SearchStationsService {
    repository: Arc<dyn StationSearchRepository>,
}
impl SearchStationsService {
    pub fn new(repository: Arc<dyn StationSearchRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(&self, name: String, country: String) -> Receiver<Result<Vec<Found>, String>> {
        let repository = self.repository.clone();
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(
                repository
                    .search(&name, &country)
                    .map_err(|error| format!("{error:#}")),
            );
        });
        receiver
    }
}

#[cfg(test)]
pub(crate) fn test_service() -> SearchStationsService {
    struct EmptyRepository;
    impl StationSearchRepository for EmptyRepository {
        fn search(&self, _: &str, _: &str) -> anyhow::Result<Vec<Found>> {
            Ok(Vec::new())
        }
    }
    SearchStationsService::new(Arc::new(EmptyRepository))
}
