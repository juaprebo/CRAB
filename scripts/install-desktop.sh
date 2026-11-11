#!/bin/sh
# Optional per-user desktop integration, called by make install.
set -efu
mode=${1:-ask}
binary=${2:-}
staging=${3:-}
types=${4:-}
case "$mode" in ask|yes|no) ;; *) echo 'ASSOCIATE must be ask, yes or no' >&2; exit 1 ;; esac
[ "$mode" != no ] || exit 0
# Packages and system-wide installs must not change a user's defaults.
if [ -n "$staging" ] || [ "$(uname -s)" != Linux ] || [ "$(id -u)" = 0 ]; then
    printf '%s\n' 'Desktop associations skipped (staged, system or non-Linux installation).'
    exit 0
fi
if [ "$mode" = ask ]; then
    if [ ! -t 0 ] || [ ! -t 1 ]; then
        printf '%s\n' 'Desktop associations skipped. Use make desktop to enable them later.'
        exit 0
    fi
    while :; do
        printf 'Use CRAB to open selected audio file types on double-click? [y/N] '
        IFS= read -r answer || answer=
        case "$answer" in
            y|Y|yes|YES|s|S|si|SI|sí) break ;;
            ''|n|N|no|NO) exit 0 ;;
            *) printf '%s\n' 'Please answer yes or no.' ;;
        esac
    done
fi
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
external=0
if command -v ffmpeg >/dev/null 2>&1 && ffmpeg -version >/dev/null 2>&1; then external=1; fi
available=$(awk -F '\t' -v external="$external" '$3 == "native" || external == 1' "$script_dir/audio-formats.tsv")
normalize_types() {
    choice=$(printf '%s' "$1" | tr ',;' '  ' | tr '[:upper:]' '[:lower:]' | awk '{$1=$1; print}')
    case "$choice" in
        all) choice=$(printf '%s\n' "$available" | awk -F '\t' '{print $1}') ;;
        none) return 2 ;;
    esac
    [ -n "$choice" ] || choice=mp3
    for type in $choice; do
        if ! printf '%s\n' "$available" | awk -F '\t' -v type="$type" '$1 == type { found=1 } END { exit !found }'; then
            printf 'Unknown or unavailable audio type: %s\n' "$type" >&2
            return 1
        fi
    done
    selected=$(printf '%s\n' $choice | sort -u | tr '\n' ' ')
}
if [ -z "$types" ] && [ "$mode" = ask ]; then
    printf '%s\n' 'Available audio types:'
    printf '%s\n' "$available" | awk -F '\t' '{ printf "  %-7s %s%s\n", $1, $2, ($3 == "ffmpeg" ? " [FFmpeg]" : "") }'
    while :; do
        printf 'Types (comma-separated IDs, all or none) [mp3]: '
        IFS= read -r types || types=none
        if normalize_types "$types"; then break; else
            status=$?
            [ "$status" != 2 ] || exit 0
        fi
    done
else
    if normalize_types "$types"; then :; else
        status=$?
        [ "$status" != 2 ] || exit 0
        exit 1
    fi
fi
all_mimes=$(printf '%s\n' "$available" | awk -F '\t' '{ n=split($5, values, " "); for(i=1;i<=n;i++) if(!seen[values[i]]++) printf "%s;", values[i] }')
selected_mimes=$(printf '%s\n' "$available" | awk -F '\t' -v selected=" $selected" 'index(selected, " " $1 " ") { n=split($5, values, " "); for(i=1;i<=n;i++) if(!seen[values[i]]++) print values[i] }')
command -v xdg-mime >/dev/null 2>&1 || { echo 'xdg-mime is required; install xdg-utils and run make desktop.' >&2; exit 1; }
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
[ -x "$binary" ] || { printf 'CRAB executable not found: %s\n' "$binary" >&2; exit 1; }
# Desktop Exec has its own quoting rules, separate from shell quoting.
quote_exec() {
    printf '"'
    printf '%s' "$1" | sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/\$/\\\\$/g; s/`/\\\\`/g; s/%/%%/g'
    printf '"'
}
if printf '%s' "$binary" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null; then
    echo 'The installation path contains unsupported control characters.' >&2
    exit 1
fi
terminal=true
exec_line="$(quote_exec "$binary") %F"
if kitty=$(command -v kitty 2>/dev/null); then
    terminal=false
    exec_line="$(quote_exec "$kitty") --class crab --title CRAB $exec_line"
fi
applications_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$applications_dir"
# Validate outside the watched applications tree. Publish by renaming a
# non-.desktop temporary file on the destination filesystem.
validation_dir=$(mktemp -d)
temp="$validation_dir/crab.desktop"
staged=
cleanup() {
    rm -f "$temp"
    [ -z "$staged" ] || rm -f "$staged"
    rmdir "$validation_dir"
}
trap cleanup 0
cat > "$temp" <<EOF
[Desktop Entry]
Type=Application
Name=CRAB
GenericName=Audio Player
Comment=Open audio files in CRAB
Exec=$exec_line
Icon=audio-x-generic
Terminal=$terminal
Categories=AudioVideo;Audio;Player;
MimeType=$all_mimes
StartupNotify=false
EOF
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$temp"
fi
staged=$(mktemp "$applications_dir/.crab-XXXXXX.tmp")
cat "$temp" > "$staged"
chmod 644 "$staged"
mv -f "$staged" "$applications_dir/crab.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications_dir"
fi
for mime in $selected_mimes; do
    xdg-mime default crab.desktop "$mime"
done
for mime in $selected_mimes; do
    actual=$(xdg-mime query default "$mime")
    [ "$actual" = crab.desktop ] || { printf 'Could not confirm association for %s: %s\n' "$mime" "$actual" >&2; exit 1; }
done
printf 'CRAB registered for: %s\n' "$selected"
