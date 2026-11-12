# CRAB architecture

CRAB is organized by feature module. Every module has its own `domain`, `application`, `infrastructure` and `presentation` directories. There are no global layer directories.

## Module ownership

| Module | Owns |
| --- | --- |
| `library` | Tracks, the library catalog, favorites, filtering, sorting, scanning, file renaming, library browsing and folder selection |
| `playlists` | Playlists, naming and insertion rules, transactional playlist operations, TOML storage, M3U interchange and playlist views |
| `playback` | Repeat and progression policy, track preparation and ReplayGain, audio decoding/output, queue/player/lyrics views, Spotify, scrobbling and desktop audio integration |
| `radio` | Stations, the minimum-station rule, station management and search services, streaming, Radio Browser access and station views |
| `localization` | Supported languages, the language persistence contract, language-change service, catalog adapter, translated messages and the language selector |
| `settings` | Saved playback preferences, the state persistence contract and service, config/state TOML adapters, theme specifications and theme presentation |
| `shell` | Runtime contract and run-loop service, module installation, terminal/input adapters, navigation, common widgets and the composed screen |

For example:

```text
src/library/
  domain/
    library.rs                 # Track data, catalog and favorite/filter rules
    track_repository.rs        # Scan result and repository contract
    file_name.rs               # Rename validation
    file_operations.rs         # Filesystem contract
  application/
    library_service.rs         # Catalog operations and asynchronous scan requests
    rename_file_service.rs     # Rename and rollback orchestration
  infrastructure/
    scanner.rs                 # FileSystemTrackRepository, tags and file watching
    file_operations.rs         # Move files without overwriting
  presentation/
    browser.rs                 # Library navigation and display projections
    folder_picker.rs           # Folder-selection input state
    views.rs                   # Library and folder-picker rendering
```

## Layer boundaries

**Domain** contains entities, policies, value validation and contracts. It has no terminal, filesystem, network or audio-backend dependencies. File paths are track identifiers, not a request to open a file.

**Application** contains services and queries that use domain objects and injected contracts. These services do not reference `App`, Ratatui, Crossterm, TOML files, Reqwest or Rodio. Examples are `PlaylistService`, `RenameFileService`, `PrepareTrackService`, `NextTrackQuery`, `ChangeSettingsService` and `ChangeLanguageService`.

**Infrastructure** implements the contracts using actual storage, audio and network APIs. `TomlPlaylistRepository`, `FileSystemTrackRepository`, `RadioBrowserRepository`, `RodioDecoder` and `ConfigLanguageRepository` are concrete adapters. An adapter does not decide how its result is rendered.

**Presentation** formats and renders data, manages selection/dialog state, translates messages and dispatches user actions to services. The composed `App` and its controllers live in `shell/presentation/controller`; their job is terminal state and coordination across module services. Feature-specific views live in their own module, rather than in one global renderer.

`src/shell/infrastructure/modules.rs` is the composition root, corresponding to an installer: it constructs adapters and injects them into the application services once. The terminal runtime implements the shell's runtime contract; `RunPlayerService` only drives that contract. Cross-module application code depends on domain types, not another module's private adapters.

Contracts exist at useful I/O boundaries. There is no interface per class, dependency container, service locator or event bus. Add one when an actual use case needs it.

## Persistence and failure behavior

Library scans replace the catalog while preserving favorites. Renaming updates the library's track/favorite reference together. `RenameFileService` validates the name, moves the file without overwriting, persists playlist references and restores the old name if that persistence fails. The presentation controller then refreshes queue entries, selections and the saved playback path. Exported M3U files and companion lyrics files remain outside that operation.

Playlist additions, renames and removals save an updated copy before replacing the visible collection. Station additions and removals follow the same policy. A failed write leaves the existing collection unchanged. Domain errors retain their types, so presentation can translate them without parsing user-supplied names.

Theme, visualizer, clock, equalizer and key-binding changes also pass through `ChangeSettingsService` and the injected `PreferencesRepository`. All existing TOML formats and command/key tokens remain compatible.

## Translations

`:language` opens the selector in the **Commands** help section. `:language es` and `:language en` apply a choice directly. `ChangeLanguageService` persists the choice before changing the active language; failed persistence leaves the language unchanged. Selection takes effect immediately and survives restart.

English is the default and the fallback for missing translations or unsupported saved languages. Unsupported interactive choices report an error and do not overwrite the saved choice. `locales/en.json` and `locales/es.json` map source messages to translations. Named placeholders are interpolated once; argument text containing braces cannot become another placeholder. Track metadata, playlist names, paths, command tokens and shortcut keys are not translated. Operating-system and dependency diagnostics may retain their original language.

To add a language, extend `localization/domain/language.rs`, add its catalog and register it in `localization/infrastructure/catalog.rs`. Add source messages to the catalogs and use the localization presentation helpers where the message is displayed. Keep all placeholders in the translation. Catalog tests validate matching keys and placeholders.

## Verification

```sh
python3 scripts/check-architecture.py
cargo fmt --check
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline --no-default-features
python3 scripts/test-integration.py
```

The architecture check rejects global layers and outward dependencies from domain/application. Business infrastructure cannot depend on presentation; the shell terminal/composition adapters are the outer integration boundary.

Application-service tests use in-memory repositories and decoder/runtime doubles. They cover failed writes, language selection, station rules, favorites, ReplayGain and playback progression without a real terminal, network or audio device. The Linux integration runner creates temporary XDG directories and an ALSA null output; it checks actual application input, file references, statistics, language persistence, restart and dialog rendering. It leaves the user's library and configuration untouched.

FFmpeg and live-stream tests remain opt-in because they require external codecs or a network stream.
