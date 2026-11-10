#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

features=()
for arg in "$@"; do
    case "$arg" in
        --no-spotify) features=(--no-default-features) ;;
        -h|--help)
            echo "Usage: ./build.sh [--no-spotify]"
            echo "Builds target/release/crab. --no-spotify leaves the Spotify support out."
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 1
            ;;
    esac
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found. Install Rust from https://rustup.rs (1.89 or newer)." >&2
    exit 1
fi

if [ "$(uname -s)" = "Linux" ]; then
    if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists alsa; then
        echo "The ALSA development files are missing." >&2
        if command -v dnf >/dev/null 2>&1; then
            echo "Install them with: sudo dnf install alsa-lib-devel pkgconf-pkg-config" >&2
        elif command -v apt-get >/dev/null 2>&1; then
            echo "Install them with: sudo apt install libasound2-dev pkg-config" >&2
        elif command -v pacman >/dev/null 2>&1; then
            echo "Install them with: sudo pacman -S alsa-lib pkgconf" >&2
        elif command -v zypper >/dev/null 2>&1; then
            echo "Install them with: sudo zypper install alsa-devel pkg-config" >&2
        else
            echo "Install the ALSA development package and pkg-config for your distribution." >&2
        fi
        exit 1
    fi
fi

cargo build --release --locked ${features[@]+"${features[@]}"}
echo "Built target/release/crab"
