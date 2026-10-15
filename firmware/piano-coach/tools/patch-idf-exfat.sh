#!/usr/bin/env bash
# Enable exFAT and GPT in FatFs, whose flags have no ESP-IDF Kconfig option.
# Accept an explicit IDF checkout; without arguments retain the Tab5 helper's
# behavior of patching downloaded IDF checkouts. Never formats removable media.
set -euo pipefail
client_dir="$(cd "$(dirname "$0")/.." && pwd)"
if [[ $# -gt 0 ]]; then
    idf_roots=("$@")
else
    shopt -s nullglob
    idf_roots=("$client_dir"/.embuild/espressif/esp-idf/*)
fi
if [[ ${#idf_roots[@]} -eq 0 ]]; then
    echo "patch-idf-exfat.sh: no ESP-IDF checkout found" >&2
    exit 1
fi
for idf_root in "${idf_roots[@]}"; do
    python3 - "$idf_root/components/fatfs/src/ffconf.h" <<'PY'
from pathlib import Path
import re
import sys
path = Path(sys.argv[1])
source = path.read_text()
updated = source
for name in ('FF_FS_EXFAT', 'FF_LBA64'):
    updated, count = re.subn(r'^(#define\s+' + name + r'\s+)[01]\b', r'\g<1>1', updated, flags=re.M)
    if count != 1:
        sys.exit(f'Unexpected {name} definition in {path}; refusing to patch')
if updated != source:
    path.write_text(updated)
    print(f'Enabled FatFs exFAT/GPT: {path}')
PY
done
