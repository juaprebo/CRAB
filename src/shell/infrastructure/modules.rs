use crate::library::{
    application::library_service::LibraryService, domain::library::Library,
    infrastructure::scanner::FileSystemTrackRepository,
};
use crate::library::{
    application::rename_file_service::RenameFileService,
    infrastructure::file_operations::FileSystemFileOperations,
};
use crate::localization::{
    application::change_language_service::ChangeLanguageService,
    infrastructure::config_language_repository::ConfigLanguageRepository,
};
use crate::playback::{
    application::prepare_track_service::PrepareTrackService,
    infrastructure::{engine::Engine, rodio_decoder::RodioDecoder},
};
use crate::playlists::{
    application::playlist_service::PlaylistService,
    infrastructure::toml_repository::TomlPlaylistRepository,
};
use crate::radio::{
    application::search_stations_service::SearchStationsService,
    infrastructure::search_repository::RadioBrowserRepository,
};
use crate::radio::{
    application::station_service::StationService,
    infrastructure::station_repository::TomlStationRepository,
};
use crate::settings::{
    application::change_settings_service::ChangeSettingsService,
    infrastructure::preferences_repository::TomlPreferencesRepository,
};
use crate::settings::{
    application::playback_state_service::PlaybackStateService, domain::playback_state::State,
    infrastructure::state_repository::TomlStateRepository,
};
use std::sync::Arc;

pub struct InstalledModules {
    pub library: LibraryService,
    pub rename_file: RenameFileService,
    pub stations: StationService,
    pub search_stations: SearchStationsService,
    pub state: PlaybackStateService,
    pub settings: ChangeSettingsService,
    pub saved: State,
    pub playlists: PlaylistService,
    pub language: ChangeLanguageService,
    pub prepare_track: PrepareTrackService<RodioDecoder>,
}

pub fn install(engine: &Engine) -> InstalledModules {
    let state = PlaybackStateService::new(Box::new(TomlStateRepository));
    let saved = state.load();
    InstalledModules {
        rename_file: RenameFileService::new(Box::new(FileSystemFileOperations)),
        library: LibraryService::new(
            Library::new(saved.favorites.iter().cloned()),
            Arc::new(FileSystemTrackRepository),
        ),
        search_stations: SearchStationsService::new(Arc::new(RadioBrowserRepository)),
        stations: StationService::new(Box::new(TomlStationRepository)),
        settings: ChangeSettingsService::new(Box::new(TomlPreferencesRepository)),
        state,
        saved,
        playlists: PlaylistService::new(Box::new(TomlPlaylistRepository)),
        language: ChangeLanguageService::new(Box::new(ConfigLanguageRepository)),
        prepare_track: PrepareTrackService::new(RodioDecoder::new(engine.tap(), engine.eq.clone())),
    }
}
