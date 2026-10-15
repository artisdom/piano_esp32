#!/usr/bin/env bash
# cargo runner: flash the ELF with the bootloader and partition table from
# the same ESP-IDF build. espflash for most boards; ESP-IDF's esptool where
# espflash cannot package the image (ESP32-S31 PSRAM-XIP) — chosen by the
# board's FLASHER (boards/<board>/board.env, exported by tools/board.sh).
set -euo pipefail
elf="$1"
shift
dir="$(dirname "$elf")"
if [[ "${PIANO_FLASHER:-espflash}" == idf ]]; then
    exec python3 "$(dirname "$0")/tools/flash-idf.py" "$elf" "$@"
fi
# esp-idf-sys copies them to <profile>/build/ (older layouts: next to the ELF).
[[ -f "$dir/build/bootloader.bin" ]] && dir="$dir/build"
exec espflash flash --monitor \
    --bootloader "$dir/bootloader.bin" \
    --partition-table "$dir/partition-table.bin" \
    "$elf" "$@"
