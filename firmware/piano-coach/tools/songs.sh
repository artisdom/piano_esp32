#!/usr/bin/env bash
# Song pack for boards without a microSD card (UI=mini profiles):
#
#   tools/songs.sh <board> build [library dir]   # pack + boards/<board>/glyphs.slint
#   tools/songs.sh <board> flash --port <port>   # write the pack to the `songs` partition
#
# `build` packs the MIDI library (default ~/w/piano/midi-piano/assets/midi or
# $PIANO_MIDI_DIR) with the board's SONGPACK_ARGS (size budget, note limit,
# preferred folders) into target/<board>/songs.bin, and regenerates the
# board's glyph list; rebuild the firmware afterwards if that file changed.
# `flash` writes the pack at the `songs` partition's offset; the firmware
# itself is flashed separately (tools/board.sh <board> run).
set -euo pipefail

app_dir="$(cd "$(dirname "$0")/.." && pwd)"
core_dir="$app_dir/../piano-core"
board="${1:-}"
cmd="${2:-}"
[[ -n "$board" && -f "$app_dir/boards/$board/board.env" && -n "$cmd" ]] || {
    echo "usage: $0 <board> build [library dir] | flash --port <port> [espflash options]" >&2
    exit 2
}
shift 2

SONGPACK_ARGS= MCU=
# shellcheck source=/dev/null
source "$app_dir/boards/$board/board.env"
parts="$app_dir/boards/$board/partitions.csv"
[[ -f "$parts" ]] && grep -q '^songs,' "$parts" || {
    echo "$0: boards/$board has no songs partition" >&2
    exit 2
}
pack="$app_dir/target/$board/songs.bin"

case "$cmd" in
build)
    lib="${1:-${PIANO_MIDI_DIR:-$HOME/w/piano/midi-piano/assets/midi}}"
    mkdir -p "$(dirname "$pack")"
    # shellcheck disable=SC2086  # SONGPACK_ARGS is a word list
    eval "args=($SONGPACK_ARGS)"
    (cd "$core_dir" && cargo +stable run -q --release --bin songpack -- \
        "$lib" -o "$pack" --glyphs "$app_dir/boards/$board/glyphs.slint" "${args[@]}")
    size=$(awk -F', *' '$1=="songs" {print $5}' "$parts")
    printf '%s: %d bytes of %d in the songs partition\n' "$pack" "$(stat -c %s "$pack")" "$((size))"
    ;;
flash)
    [[ -f "$pack" ]] || { echo "$0: no $pack — run '$0 $board build' first" >&2; exit 2; }
    offset=$(awk -F', *' '$1=="songs" {print $4}' "$parts")
    exec espflash write-bin --chip "$MCU" "$@" "$offset" "$pack"
    ;;
*)
    echo "$0: unknown command $cmd" >&2
    exit 2
    ;;
esac
