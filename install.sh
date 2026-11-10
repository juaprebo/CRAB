#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

prefix="${PREFIX:-$HOME/.local}"
build_args=()
uninstall=0

for arg in "$@"; do
    case "$arg" in
        --no-spotify) build_args+=(--no-spotify) ;;
        --uninstall) uninstall=1 ;;
        -h|--help)
            echo "Usage: ./install.sh [--no-spotify] [--uninstall]"
            echo "Builds CRAB and installs it to \$PREFIX/bin (default: ~/.local/bin)."
            echo "Example: sudo PREFIX=/usr/local ./install.sh"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 1
            ;;
    esac
done

target="$prefix/bin/crab"

if [ "$uninstall" -eq 1 ]; then
    if [ -e "$target" ]; then
        rm -f "$target"
        echo "Removed $target"
    else
        echo "Nothing to remove at $target"
    fi
    echo "Your settings in ~/.config/crab were left in place."
    exit 0
fi

./build.sh ${build_args[@]+"${build_args[@]}"}

mkdir -p "$prefix/bin"
install -m 755 target/release/crab "$target"
echo "Installed $target"

case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Note: $prefix/bin is not in your PATH. Add it to your shell profile to run 'crab' from anywhere." ;;
esac
