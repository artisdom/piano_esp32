#!/usr/bin/env bash
# Build / flash Piano Coach for one board:
#
#   tools/board.sh <board> [build|run|check|clippy] [cargo options...]
#   tools/board.sh korvo run --release -- --port /dev/ttyUSB3
#
# Boards are the directories under boards/ that contain a board.env; that
# file declares the Cargo feature, MCU, ESP-IDF source and flasher. Everything
# that has to agree for a board is set here together; build.rs cross-checks
# the feature against MCU so a mismatched build fails fast.
set -euo pipefail

app_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$app_dir"

boards() { for f in boards/*/board.env; do basename "$(dirname "$f")"; done | paste -sd' '; }
usage() {
    echo "usage: $0 <board> [build|run|check|clippy] [cargo options...]" >&2
    echo "boards: $(boards)" >&2
    exit 2
}

board="${1:-}"
[[ -n "$board" && -f "boards/$board/board.env" ]] || usage
command="${2:-build}"
shift $(( $# >= 2 ? 2 : 1 ))

FEATURE= MCU= ESP_IDF_VERSION= IDF_PATH_DEFAULT= PICOLIBC= PATCH_EXFAT= FLASHER=
# shellcheck source=/dev/null
source "boards/$board/board.env"

# Shared ESP-IDF / tool downloads (multi-GB) — reuse the existing herdline
# install unless the caller points elsewhere.
export ESP_IDF_TOOLS_INSTALL_DIR="${ESP_IDF_TOOLS_INSTALL_DIR:-custom:/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif}"
export MCU PIANO_BOARD="$board" PIANO_FLASHER="$FLASHER"

if [[ -n "$ESP_IDF_VERSION" ]]; then
    export ESP_IDF_VERSION
    unset IDF_PATH || true
else
    export IDF_PATH="${IDF_PATH:-$IDF_PATH_DEFAULT}"
    if [[ ! -f "$IDF_PATH/tools/idf.py" ]]; then
        echo "$0: IDF_PATH=$IDF_PATH is not an ESP-IDF checkout" >&2
        exit 2
    fi
fi
if [[ "$PATCH_EXFAT" == 1 ]]; then
    "$app_dir/tools/patch-idf-exfat.sh" "$IDF_PATH"
fi
if [[ "$PICOLIBC" == 1 ]]; then
    export RUSTFLAGS="${RUSTFLAGS:---cfg espidf_time64} --cfg espidf_picolibc"
fi

export ESP_IDF_SYS_EXTRA_COMPONENTS_FILE="$app_dir/boards/$board/esp-idf-components.toml"
export ESP_IDF_SDKCONFIG_DEFAULTS="$app_dir/boards/$board/sdkconfig.defaults"
export ESP_IDF_SDKCONFIG="$app_dir/sdkconfig.$board"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$app_dir/target/$board}"

exec cargo "$command" --no-default-features --features "$FEATURE" "$@"
