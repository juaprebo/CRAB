<p align="center"><img src="docs/logo.jpg" alt="CRAB - Rust Audio Box" width="280"></p>

# CRAB - Rust Audio Box

A terminal audio player written in Rust. It plays local files, internet radio and Spotify, and draws a live spectrum analyzer of whatever is playing.

![CRAB playing a local library](docs/library.png)

## Features

- Local library with automatic reload when files are added or removed, and a built-in folder browser
- Internet radio with live track titles (ICY metadata) and automatic reconnection
- Spotify through Spotify Connect (optional, see the notice below)
- Real-time spectrum bars, with configurable width, gap and peak markers
- Themes: ten built-in ones plus your own, made in the built-in theme editor or defined in a TOML file
- Volume follows the system output volume, so changes made outside CRAB show up

## Install

You need Rust 1.89 or newer and the ALSA development headers.

| Distribution | Packages |
| --- | --- |
| Debian / Ubuntu | `libasound2-dev pkg-config` |
| Fedora | `alsa-lib-devel` |
| Arch | `alsa-lib` |

Then build and install it to `~/.local/bin`:

    ./install.sh

Or only build it and run it from the repository:

    ./build.sh
    ./target/release/crab ~/Music

Spotify support is a Cargo feature that is on by default. Pass `--no-spotify` to either script to leave it out. `PREFIX` changes the install location (for example `sudo PREFIX=/usr/local ./install.sh`) and `./install.sh --uninstall` removes the binary. The scripts wrap plain `cargo build --release --locked`, which works too.

## Usage

    crab [PATH...]   start the player scanning the given folders or files
    crab login       sign in to Spotify (Premium account required)
    crab --help

Without arguments CRAB opens the folders you used last time, or `~/Music` the first time.

| Key | Action |
| --- | --- |
| Up / Down, `j` / `k` | select |
| Enter | play |
| Space | pause or resume |
| `n` / `p` | next / previous |
| Left / Right | seek 5 seconds (local files) |
| `+` / `-` | volume |
| `m` | mute |
| `s` | shuffle |
| `o` | choose a music folder |
| `/` | find radio stations (Radio tab) |
| `x` | remove the selected station, press twice (Radio tab) |
| `t` | open the theme picker |
| `r` | reload the configuration |
| Tab, `1` `2` `3` | switch between Library, Radio and Spotify |
| `q` | quit |

### Folder browser

Press `o` to browse folders without leaving CRAB. Enter opens a folder, Backspace goes up, Space makes the current folder your library, `a` adds it to the library, `/` lets you type a path, `~` jumps to your home folder and `.` shows hidden folders. The chosen folders are remembered for the next start. Folders given on the command line take priority.

## Radio

Stations live in `~/.config/crab/stations.toml`, which is created on first run:

    [[station]]
    name = "My station"
    url = "https://example.com/stream.mp3"

MP3, AAC and Ogg streams work, and `.pls` and `.m3u` playlist URLs are resolved automatically. HLS streams (`.m3u8` lists of segments) are not supported.

### Finding stations

Press `/` in the Radio tab to search the community directory at [radio-browser.info](https://www.radio-browser.info). The country field starts with the one from your locale (for example `ES` for Spanish stations), and the name field is optional: leave it empty to see the most voted stations of that country. Enter searches, Tab moves between the fields and the results, and Enter on a result saves it to `stations.toml` and starts playing it. Only streams CRAB can play are listed. Press `x` twice on a station to remove it from your list.

## Spotify

After `crab login`, CRAB appears as a Spotify Connect device named "CRAB". Open Spotify on any device and pick it from the devices menu. The audio plays through CRAB and drives the spectrum bars. CRAB has no search of its own: you choose what to play from the Spotify app.

CRAB is an independent project. It is not affiliated with, endorsed by or sponsored by Spotify. The integration is built on [librespot](https://github.com/librespot-org/librespot), an unofficial open source client, whose authors warn that using it to connect to Spotify is probably forbidden by Spotify's terms and that you use it at your own risk. A Spotify Premium account is required. If you are not comfortable with that, build CRAB without the `spotify` feature.

## Configuration

`~/.config/crab/config.toml` is created on first run. Press `r` inside CRAB to reload it.

    theme = "crab"
    show_art = true
    art_strength = 0.35
    art_file = "~/.config/crab/art.txt"

    [visualizer]
    bar_width = 2
    gap = 1
    peaks = true

    [themes.mine]
    base = "nord"
    accent = "#ff8800"
    bars = ["#5e81ac", "#88c0d0", "#ebcb8b"]

Built-in themes: `crab`, `synthwave`, `ocean`, `matrix`, `nord`, `dracula`, `gruvbox`, `mono`, `paper` and `rustbox`.

### Theme picker and editor

![Theme picker](docs/themes.png)

Press `t` to open the theme picker. Moving through the list previews each theme live, Enter keeps the one you chose and saves it to `config.toml`, and Esc goes back to the previous one.

Press `n` in the picker to generate a whole theme from a single color. Choose the accent with the arrow keys (left and right change the hue, up and down the lightness, PgUp and PgDn the saturation), switch between a dark and a light variant with `m`, and the interface updates as you go. Enter takes you to the editor to fine tune it. Press `e` in the picker to edit the highlighted theme instead, or `x` to delete one of your own themes (it asks for confirmation). Built-in themes cannot be deleted.

![Generating a theme from one color](docs/generator.png)

In the editor, Enter on a color opens the same color picker, and `#` lets you type a hex value such as `#ff5a4e`. `d` clears the optional `background` and `art` colors, `+` and `-` add or remove colors of the bar gradient (2 to 6), and `s` asks for a name and writes the theme to the `[themes.<name>]` section of `config.toml`, leaving the rest of the file untouched. Esc discards the changes.

![Theme editor](docs/editor.png)

A custom theme can set `base`, `background`, `text`, `dim`, `border`, `accent`, `secondary`, `on_accent`, `error`, `ok`, `art` and `bars`, a list of two or more colors that form the bar gradient. Anything you leave out is inherited from the base theme. Defining a theme with a built-in name overrides it.

A crab is drawn behind the lists and scales with the terminal size. `art_strength` (0.05 to 1.0) sets how bright it is, `show_art = false` hides it, and `art_file` replaces it with any text file, centered behind the lists.

## Files

| Path | Purpose |
| --- | --- |
| `~/.config/crab/config.toml` | themes and display options |
| `~/.config/crab/stations.toml` | radio stations |
| `~/.local/state/crab/library_roots.txt` | folders remembered from the folder browser |
| `~/.cache/crab/crab.log` | error log |
| `~/.cache/crab/spotify/` | saved Spotify credentials |

## Support

CRAB is free and open source. If you enjoy it and want to give something back, you can [buy me a coffee on Ko-fi](https://ko-fi.com/juaprebo). Donations are voluntary, unlock nothing and have no relation to the Spotify integration.

## Contributing

Issues and pull requests are welcome. Run `cargo fmt` and `cargo test` before sending a change. Unless you state otherwise, any contribution you submit is licensed under the same terms as the project, as described below.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
