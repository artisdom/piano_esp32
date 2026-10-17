# Dev-kit hardware reference

One document per board Piano Coach runs on. Each document is meant to be
enough to develop for that board without the vendor's material: identity,
every on-board part, a pin map, each subsystem, partitions and sdkconfig,
how to build / flash / read the console, what is verified at the piano,
and the quirks found on the bench. Board-specific vendor files (schematics,
specs, dimensions) are copied next to them under `vendor/<board>/`; the
vendor folders of the Tab5 (`doc/M5Stack-Tab5/`), the ESP32-1732S019
(`doc/1.9inch_ESP32-1732S019/`) and the ESP32-2424S012
(`doc/1.28inch_ESP32-2424S012/`) were already in `doc/`. Generic chip and
module datasheets are not copied; each document names them.

How facts are labelled: pins and parts that the firmware drives are taken
from the code (`firmware/piano-coach/src/board/<board>/`, the C BSPs in
`firmware/piano-coach/components/`, `boards/<board>/sdkconfig.defaults`)
and are known to work. Facts that only come from a schematic or vendor
spec say so ("schematic, not used by firmware"). Where code and vendor
documents disagree, the document says which one the firmware follows.

| Board | Document | MCU (Rust target) | Display / UI class | Piano link | Songs | Console port |
|---|---|---|---|---|---|---|
| M5Stack Tab5 | [tab5.md](tab5.md) | ESP32-P4 + ESP32-C6 (riscv32imafc) | 5″ 720×1280 MIPI-DSI touch, shown landscape / full | USB-A host or BLE via the C6 | microSD | USB Serial/JTAG 303a:1001 |
| ESP32-S31-Korvo-1 V1.1 | [korvo.md](korvo.md) | ESP32-S31 (riscv32imafc) | 800×480 RGB touch / full @0.625 | USB-A host (HS) or BLE | microSD | CP2102N (10c4) |
| ESP32-1732S019 | [1732s019.md](1732s019.md) | ESP32-S3 (xtensa, `esp198`) | 1.9″ 320×170 SPI / mini | BLE | flash pack, all 2449 | CH340C |
| ESP32-2424S012C | [2424s012.md](2424s012.md) | ESP32-C3 (riscv32imc) | 1.28″ 240×240 round touch / mini | BLE | flash pack, 776 | USB Serial/JTAG 303a:1001 |
| LILYGO T-Display-S3 | [tdisplays3.md](tdisplays3.md) | ESP32-S3 (xtensa, `esp198`) | 1.9″ 320×170 i80 / mini | BLE | flash pack, all | USB Serial/JTAG |
| ESP-WROVER-KIT v4.1 | [wroverkit.md](wroverkit.md) | ESP32 rev 1 (xtensa, `esp198`) | 3.2″ 320×240 / mini @1.25 | BLE | flash pack, 776 | FT2232HL interface 1 (`ttyUSB1`) |
| Ai-Thinker ESP32-Audio-Kit | [audiokit.md](audiokit.md) | ESP32 rev 3 (xtensa, `esp198`) | none / headless (6 keys, phone page) | BLE | microSD | CP2102 10c4:ea60 |
| VCC-GND YD-ESP32-S3 N16R8 | [yds3.md](yds3.md) | ESP32-S3 (xtensa, `esp198`) | none / headless (BOOT key, RGB LED, phone page) | USB-C OTG and BLE at once | flash pack, all | CH343 1a86:55d3 |
| ESP32-2432S028R (CYD) | [2432s028.md](2432s028.md) | ESP32 (xtensa, `esp198`) | 2.8″ 320×240 resistive touch / mini | BLE | flash pack, 776 | CH340 1a86:7523 |

Ports change when boards are re-plugged; identify a board by its USB
serial string or MAC (`udevadm info -q property -n /dev/ttyACMx | grep
ID_SERIAL`, `espflash board-info --port …`) and prefer the stable
`/dev/serial/by-id/…` path. Each document lists the bench unit's
identifiers.

## Common to every board

* **The piano**: GZUT 88-key LED piano, USB-MIDI device `GZU-tek
  CHMidi-2.3` (5022:0716) and BLE peripheral `GZUT-MIDI` — protocol in
  [../GZUT_piano_protocol.md](../GZUT_piano_protocol.md). It accepts one
  BLE central at a time: power other boards (and tablets) off when testing
  one.
* **Firmware structure**: the board contract (`src/board/contract.rs`),
  board profiles (`boards/<board>/`) and the porting checklist are in
  [../../firmware/piano-coach/boards/README.md](../../firmware/piano-coach/boards/README.md);
  architecture, user guide and hardware results in
  [../piano_coach_firmware.md](../piano_coach_firmware.md); orientation and
  bench procedures in [../../AGENTS.md](../../AGENTS.md).
* **Build / flash**: `tools/board.sh <board> build --release`, then
  `espflash flash --chip <mcu> --port P --baud 921600 --bootloader
  T/bootloader.bin --partition-table T/partition-table.bin T/piano-coach`
  with `T=target/<board>/<target>/release` (Korvo: `tools/flash-idf.py`).
  Flashing the app keeps NVS settings and the song pack partition; the
  pack is written separately with `tools/songs.sh <board> flash`.
* **Serial logs**: on USB-UART bridges (CP210x, CH34x, FT2232) open the
  port with DTR/RTS false (else the board resets); holding RTS keeps the
  board in reset. On native USB Serial/JTAG ports (C3, S3, P4) opening the
  port from pyserial can reset the chip or drop it into download mode —
  use `espflash monitor` (it resets the board once) and `espflash reset`
  to recover.
* **Original ESP32 boards** (CYD, WROVER-KIT, Audio Kit) must keep
  `CONFIG_BT_NIMBLE_HS_FLOW_CTRL=n`, or BLE stops delivering the piano's
  key presses after ~24 packets (see each document and the firmware doc).
