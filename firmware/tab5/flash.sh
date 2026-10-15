#!/usr/bin/env bash
# cargo runner: flash the ELF together with the bootloader and partition table
# from our own ESP-IDF build (embuild copies both next to the ELF).
set -euo pipefail
elf="$1"
shift
dir="$(dirname "$elf")"
# esp-idf-sys copies them to <profile>/build/ (older layouts: next to the ELF).
[[ -f "$dir/build/bootloader.bin" ]] && dir="$dir/build"
exec espflash flash --monitor \
    --bootloader "$dir/bootloader.bin" \
    --partition-table "$dir/partition-table.bin" \
    "$elf" "$@"
