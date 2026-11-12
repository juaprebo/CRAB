use crate::settings::domain::{playback_state::State, state_repository::StateRepository};

pub struct PlaybackStateService {
    repository: Box<dyn StateRepository>,
}
impl PlaybackStateService {
    pub fn new(repository: Box<dyn StateRepository>) -> Self {
        Self { repository }
    }
    pub fn load(&self) -> State {
        self.repository.load()
    }
    pub fn save(&self, state: &State) {
        self.repository.save(state);
    }
}
