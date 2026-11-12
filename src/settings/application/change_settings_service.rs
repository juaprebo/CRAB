use crate::settings::domain::preferences_repository::{PreferenceChange, PreferencesRepository};
use anyhow::Result;

pub struct ChangeSettingsService {
    repository: Box<dyn PreferencesRepository>,
}
impl ChangeSettingsService {
    pub fn new(repository: Box<dyn PreferencesRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(&self, change: PreferenceChange) -> Result<()> {
        self.repository.update(change)
    }
}
