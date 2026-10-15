#!/usr/bin/env python3
"""Flash an S31 PSRAM-XIP ELF built by ESP-IDF (from the herdline project).

espflash 4.5 cannot locate appdesc in S31's PSRAM-XIP address range, so the
image is converted with the matching ESP-IDF build's esptool (`elf2image`),
offsets/settings taken from the build. Writing then uses `espflash
write-bin` (its flasher stub: ~16 s for 7.5 MB) when espflash is installed;
ESP-IDF's esptool runs the S31 without a stub (~7 minutes) and is the
fallback (`--writer esptool`).
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).digest()


def check_app_size(table, offset, image_size):
    """Reject an image that would write beyond its application partition."""
    entry = struct.Struct("<HBBII16sI")
    for position in range(0, len(table) - entry.size + 1, entry.size):
        magic, kind, _, start, size, _, _ = entry.unpack_from(table, position)
        if magic == 0x50AA and kind == 0 and start == offset:
            if image_size > size:
                raise RuntimeError(f"Application image ({image_size} bytes) exceeds partition ({size} bytes)")
            return
    raise RuntimeError(f"No application partition at {offset:#x} in the matching partition table")


def build_metadata(elf):
    matches = []
    for manifest in (elf.parent / "build").rglob("flasher_args.json"):
        build = manifest.parent
        config = json.loads(manifest.read_text())
        if config["extra_esptool_args"]["chip"] != "esp32s31":
            continue
        # Refuse to combine an ELF with another build's bootloader/table.
        if all(digest(build / config[key]["file"]) == digest(elf.parent / filename)
               for key, filename in [("bootloader", "bootloader.bin"),
                                     ("partition-table", "partition-table.bin")]):
            matches.append((manifest.stat().st_mtime, build, config))
    if not matches:
        raise RuntimeError("No matching S31 ESP-IDF build beside the ELF; rebuild with tools/board.sh")
    _, build, config = max(matches, key=lambda item: item[0])
    cache = (build / "CMakeCache.txt").read_text()
    python = re.search(r"^PYTHON:[^=]+=(.+)$", cache, re.MULTILINE)
    if not python:
        raise RuntimeError(f"ESP-IDF Python not found in {build}/CMakeCache.txt")
    return build, config, python.group(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--port", "-p", required=True)
    parser.add_argument("--baud", "-b", default="921600", help="flashing baud rate")
    parser.add_argument("--no-monitor", action="store_true")
    parser.add_argument("--dry-run", action="store_true", help="print commands without flashing")
    parser.add_argument("--writer", choices=["auto", "espflash", "esptool"], default="auto",
                        help="tool that writes the converted images (default: espflash if installed)")
    args = parser.parse_args()
    elf = args.elf.resolve()
    build, config, python = build_metadata(elf)
    sdkconfig = json.loads((build / "config/sdkconfig.json").read_text())
    image = elf.with_suffix(".bin")
    esptool = [python, "-m", "esptool", "--chip", "esp32s31"]
    convert = esptool + ["elf2image", *config["write_flash_args"],
                         "--elf-sha256-offset", "0xb0", "--flash-mmu-page-size", sdkconfig["MMU_PAGE_MODE"],
                         "--min-rev-full", str(sdkconfig["ESP_REV_MIN_FULL"]),
                         "--max-rev-full", str(sdkconfig["ESP_REV_MAX_FULL"]), "-o", str(image), str(elf)]
    parts = [("bootloader", elf.parent / "bootloader.bin"),
             ("partition-table", elf.parent / "partition-table.bin"),
             ("app", image)]
    writer = args.writer
    if writer == "auto":
        writer = "espflash" if shutil.which("espflash") else "esptool"
    if writer == "espflash":
        # One write-bin per region (stub + compression); each verifies itself.
        flash = [["espflash", "write-bin", "--chip", "esp32s31", "--port", args.port,
                  "--baud", args.baud, config[key]["offset"], str(filename)] for key, filename in parts]
    else:
        cmd = esptool + ["--port", args.port, "--baud", args.baud]
        if not config["extra_esptool_args"]["stub"]:
            cmd += ["--no-stub"]
        cmd += ["write-flash", *config["write_flash_args"]]
        for key, filename in parts:
            cmd += [config[key]["offset"], str(filename)]
        flash = [cmd]
    commands = [convert, *flash]
    if not args.no_monitor:
        project = json.loads((build / "project_description.json").read_text())
        toolchain = str(Path(project["c_compiler"]).with_name("riscv32-esp-elf-"))
        commands += [[python, "-m", "esp_idf_monitor", "--port", args.port,
                      "--baud", str(sdkconfig["ESP_CONSOLE_UART_BAUDRATE"]),
                      "--target", "esp32s31", "--toolchain-prefix", toolchain, str(elf)]]
    for command in commands:
        if args.dry_run:
            print(json.dumps(command))
        else:
            if command is flash[0]:
                check_app_size((elf.parent / "partition-table.bin").read_bytes(),
                               int(config["app"]["offset"], 0), image.stat().st_size)
            subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
