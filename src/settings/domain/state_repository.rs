use super::playback_state::State;

pub trait StateRepository {
    fn load(&self) -> State;
    fn save(&self, state: &State);
}
