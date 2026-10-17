# AGENTS.md — working on piano_esp32

Notes for AI agents (and humans) picking up this project. Read this first,
then `README.md`, `doc/piano_coach_firmware.md` (user guide, build, architecture,
hardware results), `firmware/piano-coach/boards/README.md` (porting guide)
and, before touching a board's hardware, its `doc/boards/<board>.md`.

## What this is

**Piano Coach**: Rust firmware for ESP32-family boards that plays MIDI songs
on a **GZUT 88-key LED digital piano** and teaches them — the piano's red key
LEDs show what to play next. Plus the reverse-engineered piano protocol.

* `firmware/piano-core/` — board-independent Rust (std, host-testable): MIDI
  parser (`smf`), songs and hand split (`song`), the Listen/Learn/Follow
  state machine (`engine`), USB-MIDI / BLE-MIDI framing (`usbmidi`,
  `blemidi`), sheet-music engraver (`notation`), additive piano synth
  (`synth`), library naming/sorting (`library`), pinyin search (`search`),
  progress/bookmarks (`progress`), song globe (`sphere`), flash song pack
  (`songpack`, built by `src/bin/songpack.rs`), piano-key remote control
  and shuffle order (`controls`).
* `firmware/piano-coach/` — the ESP-IDF application (esp-idf-svc/hal/sys,
  vendored copies in `vendor/` from the sibling "herdline" project, Slint UI).
* `doc/` — `GZUT_piano_protocol.md` (protocol), `piano_coach_firmware.md`
  (the main firmware document), `boards/` (one hardware reference per dev
  kit: pin maps, parts, quirks; schematics in `boards/vendor/`),
  `test_log_2026-10-15.md` (raw captures), `images/` (README screenshots,
  rendered on the PC), vendor folders for the Tab5 and the two Sunton
  boards.
* Sibling repos: **`../piano-coach`** — the Android (Kotlin/Compose) port of
  the same app (`core/` mirrors piano-core: Engine.kt, Controls.kt, …;
  `Coach.kt` is the app); **`../midi-piano/assets/midi`** — the song
  library (JustPiano2-20160326: 2449 songs, 952 folders, 36 MB raw).

## The GZUT piano (facts the code relies on)

* USB-C on the piano is a USB-MIDI device `GZU-tek CHMidi-2.3` (5022:0716);
  BLE name `GZUT-MIDI`; Bluetooth audio `GZUT-MUSIC`.
* **Note On velocity 1 lights a key's LED silently** (velocities 1, 2, 8
  silent; ≥ 16 audible). Note Off / CC 123 turns LEDs off. Host LEDs ignore
  physical key presses. Everything on channel 1.
* **BLE quirk:** every write must be `80 80` + plain MIDI (header 0x80,
  timestamp 0x80); the piano drops standard timestamped BLE-MIDI packets,
  so BlueZ's MIDI plugin / Android's BLE-MIDI cannot drive it. Raw GATT
  tests on Linux need `bluetoothd -P midi` (passwordless sudo exists).
  Packets ≤ 244 bytes.
* **Only one BLE connection at a time.** If the Tab5 holds it, a tablet or
  another board cannot connect — power the others off or use USB.
* After connecting, the piano renegotiates the BLE link to 10 ms interval /
  500 ms supervision; headless boards and the original-ESP32 boards
  (`cfg(ble_relaxed)`) ask again for 30–50 ms / 10 s supervision
  (`ble_midi.rs` `relax_link`), else Wi-Fi starves.
* Panel SysEx from the piano: `F0 52 4B pp vv 00 00 F7` (volume, metronome…).

## Boards (all in `firmware/piano-coach/boards/<board>/`)

| Board dir | Board | MCU / target | UI class | Piano link | Songs | Console / flash port (last seen) |
|---|---|---|---|---|---|---|
| `tab5` | M5Stack Tab5 | ESP32-P4, riscv32imafc | full (1280×720) | USB-A host or BLE via ESP32-C6 (ESP-Hosted) | microSD | USB Serial/JTAG, Espressif 303a:1001, MAC 30:ed:a0:e2:fa:9b (`/dev/ttyACM*`) |
| `korvo` | ESP32-S31-Korvo-1 V1.1 | ESP32-S31, riscv32imafc | full (800×480 @0.625) | USB-A host (HS) or BLE | microSD | CP2102N 10c4 (`/dev/ttyUSB0`) |
| `1732s019` | ESP32-1732S019 1.9″ | ESP32-S3, Xtensa (`esp198`) | mini | BLE | flash pack, all 2449 | `/dev/ttyUSB0` |
| `2424s012` | ESP32-2424S012C 1.28″ round touch | ESP32-C3, riscv32imc | mini | BLE | flash pack, 776 (≤ 1200 notes) | USB Serial/JTAG, MAC 40:4c:ca:8d:a9:68 |
| `tdisplays3` | LILYGO T-Display-S3 | ESP32-S3, Xtensa | mini | BLE | flash pack, all | USB Serial/JTAG |
| `wroverkit` | ESP-WROVER-KIT v4.1 | ESP32 rev 1, Xtensa | mini (3.2″ @1.25) | BLE | flash pack, 776 | FT2232: `ttyUSB0` = JTAG, `ttyUSB1` = console |
| `audiokit` | Ai-Thinker ESP32-Audio-Kit (A1S, ES8388) | ESP32 rev 3, Xtensa | headless (6 keys + phone web page) | BLE | microSD | CP2102 (`/dev/ttyUSB0`) |
| `yds3` | VCC-GND YD-ESP32-S3 N16R8 | ESP32-S3, Xtensa | headless (BOOT key, WS2812, web page) | **USB-C OTG and BLE at once** (`dual.rs`) | flash pack, all | CH343 1a86:55d3 (`/dev/ttyACM*`) |

Port numbers change when things are re-plugged — identify a board with
`udevadm info -q property -n /dev/ttyACMx | grep ID_SERIAL` or
`espflash board-info --port …` before flashing. Two Android tablets are
usually on adb too: Lenovo TB-X505F (`HGT40X0C`) and F888
(`zc09202289628318`).

Board quirks worth knowing:
* **Tab5**: BLE goes through the ESP32-C6 (needs ESP-Hosted firmware with
  BT; 2.12.11 works). USB-HS host and the C6's SDIO link interfere, so one
  transport per boot (Settings switch restarts). espflash sometimes fails to
  connect once ("Error while connecting") — just retry.
* **Korvo**: needs an ESP-IDF **master** checkout (`/home/nws/w/esp32/esp-idf`,
  `FLASHER=idf`); flash with
  `IDF_PATH=/home/nws/w/esp32/esp-idf python3 tools/flash-idf.py target/korvo/riscv32imafc-esp-espidf/release/piano-coach --port /dev/ttyUSB0 --no-monitor`
  (espflash cannot package S31 PSRAM-XIP images). Its touch panel has a dead
  strip on the bench unit; the SET key cycles pages.
* **T-Display-S3**: if another firmware took over its USB port, enter
  download mode once (hold BOOT, press RST).
* **WROVER-KIT**: a microSD card in the slot blanks the LCD (card-detect
  shares GPIO21 with LCD D/C) — keep it empty. Main-task stack must be 8 KB.
* **Audio Kit**: the speaker amps hang on ES8388 LOUT2/ROUT2, which
  esp_codec_dev leaves at −30 dB, and `esp_codec_dev_write_reg` silently does
  nothing for this codec — `components/audiokit_bsp` writes 0x30/0x31 = 0x21
  through the I²C ctrl interface. Plain ESP32 internal RAM is the limit with
  Wi-Fi + BLE: `SPIRAM_MALLOC_ALWAYSINTERNAL=0`, PSRAM thread stacks
  (`threads.rs`), small Wi-Fi pools, HTTP server started before Wi-Fi.
* **ESP32-2424S012C**: GC9A01 panels differ between units — the first
  bench unit (MAC 40:4c:ca:8d:a9:68) is right unmirrored, a second
  (68:67:25:6b:92:fc) needs **Settings → Mirror display** = On (NVS key
  `lcdmirror`, already set on that unit).
* **ESP32-2432S028R (CYD, `2432s028`)**: ILI9341-class panel (reads ID
  18 06 06), landscape with the USB connector on the LEFT (MV + MY);
  XPT2046 touch bit-banged (`board/xpt2046.rs`), calibrated on the bench
  unit; CH340 console.
* **Original ESP32 (CYD, WROVER-KIT, Audio Kit) BLE:** keep
  `CONFIG_BT_NIMBLE_HS_FLOW_CTRL=n` in their sdkconfig.defaults. ESP-IDF
  defaults it on for this chip only; with it the controller stops handing
  the piano's packets to NimBLE after ~24 (the host's ACL buffer count),
  so key presses stop a second into playing, the link drops (0x208) and
  reconnects fail discovery until a reset. Found with the drop/pool
  logging in `ble_midi.rs`; fixed and verified on the CYD, Audio Kit and
  WROVER-KIT (2026-10-16).
* **YD-ESP32-S3**: the OTG port supplies the piano's VBUS only with the
  "USB-OTG" solder jumper bridged (it is on the bench unit). Console is UART0
  via the CH343 because the native USB PHY belongs to the OTG host. Holding
  RTS (pyserial `rts=True`) keeps it in reset — handy for USB-attach tests.
* **Piano USB queue** (all USB boards): key presses made while no host
  reads the piano's USB port arrive in one burst at attach; `usb_midi.rs`
  drops key events for 500 ms after attach (`STALE_MS`).

## Building, flashing, testing

```bash
cd firmware/piano-coach
tools/board.sh <board> build --release          # board = a dir under boards/
tools/board.sh <board> run --release -- --port /dev/ttyXXX   # flash + monitor (blocks)
tools/songs.sh <board> build                     # song pack (no-SD boards)
tools/songs.sh <board> flash --port /dev/ttyXXX --baud 921600
```

* `tools/board.sh` reads `boards/<board>/board.env` (FEATURE, MCU,
  ESP_IDF_VERSION or IDF_PATH_DEFAULT, TARGET, TOOLCHAIN, FLASHER, `UI=` full
  | mini | headless, `HAS=` usb sd speaker, SONGPACK_ARGS, CARGO_CONFIG) and
  sets ESP_IDF_SYS_EXTRA_COMPONENTS_FILE, sdkconfig defaults, a per-board
  `target/<board>` dir. `build.rs` turns UI/HAS into `cfg(ui_full|ui_mini|
  ui_headless)` and `cfg(has_usb|has_sd|has_speaker)`.
* Toolchains: RISC-V boards use the repo's nightly (`rust-toolchain.toml`);
  Xtensa boards use the esp-rs toolchain `esp198`
  (`espup install --name esp198 --toolchain-version 1.98.1.0`). ESP-IDF
  v5.5.3 and tools are shared from
  `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`.
* A first build of a board compiles ESP-IDF (10+ min); run long builds in
  the background. Building all eight boards takes a while — do it after any
  change to shared code (`player.rs`, `browse.rs`, `board/contract.rs`,
  piano-core). Pre-existing harmless warnings: `cst816` unused on boards
  without touch, `SD_MOUNT` on Korvo, bindgen `_atoi_r` redeclarations.
* Direct espflash for most boards (what the session used):
  `espflash flash --chip <mcu> --port P --baud 921600 --bootloader T/bootloader.bin --partition-table T/partition-table.bin T/piano-coach`
  with `T=target/<board>/<target>/release`.
* **Host tests**: `cd firmware/piano-core && cargo +stable test --release`
  (34 tests: parser, engine incl. Follow, notation, search, controls, shuffle).
* **UI on the PC**: `tests/ui_preview` (full UI; `cargo +stable run --release
  --target x86_64-unknown-linux-gnu --bin piano-tab5-ui-preview`, output in
  `$UI_PREVIEW_DIR`) and `tests/mini_preview`; README screenshots via
  `tools/make_screenshots.sh` (also renders the phone page with
  `tools/web_mock.py` + headless Chrome).
* **Serial logs**: open the port with pyserial with DTR/RTS false (else you
  reset the board); reset via RTS pulse. Don't `pkill -f <pattern>` that
  matches your own shell command line (it kills the shell) — use PID files.
* **Set a pref from the PC** (e.g. a board too mirrored to navigate): build
  an NVS image with ESP-IDF's
  `components/nvs_flash/nvs_partition_generator/nvs_partition_gen.py generate prefs.csv nvs.bin 0x6000`
  from a CSV `key,type,encoding,value` / `piano,namespace,,` /
  `lcdmirror,data,i32,1` (prefs are i32 in namespace `piano`; booleans
  0/1) and `espflash write-bin --port P 0x9000 nvs.bin` (the `nvs`
  partition offset in the board's partitions.csv). It replaces all saved
  prefs and BLE bonds on that board.
* **Wi-Fi tests of headless boards**: the PC's dongle `wlxa0d768003aa5` is
  normally on the home network `Trustpower_5.0GHz_0213`. Join the board's AP
  (`PianoCoach-XXXX`, password `pianocoach`, page at http://192.168.4.1) with
  `nmcli dev wifi connect … ifname wlxa0d768003aa5`, set the profile to
  `autoconnect no` / `ipv4.never-default yes`, and reconnect the home network
  afterwards.

## Architecture in one page (firmware)

* `main.rs`: board early init → prefs (NVS) → transport (USB host, BLE, or
  both via `dual.rs` on headless+USB boards) → speaker synth (`audio.rs`) →
  player thread (`player.rs`) → the UI class's thread.
* **Board boundary**: `src/board/<board>/mod.rs` + `src/board/contract.rs`
  (compile-time check of the items each class needs) + `bindings.h` branch +
  optional C component in `components/`. Nothing outside `src/board/` names a
  board. See `boards/README.md` for the porting checklist.
* **UI classes**: `ui.rs` + `ui/*.slint` (full: library, play page with
  falling notes / sheet music, settings, search, song globe), `ui_mini.rs` +
  `ui/mini/app.slint` (wheel menu, play screen, band-rendered), `headless.rs` +
  `web.rs` + `web/index.html` (keys, LED, Wi-Fi AP with captive portal, JSON
  API, gzipped page).
* **player.rs** owns the `Engine`, routes MIDI to piano and/or synth
  ("Notes sound from"), publishes a `Snapshot`, and turns piano key presses on
  control keys into `Control`s for the UI.
* **browse.rs**: the library as one folder tree whether on microSD or in the
  flash pack (virtual root `/songs`, pack songs load as `/pack/<index>`); song
  / folder stepping, random song, the 儿时回忆 boot song, `FolderShuffle`.
* **Engine modes** (`piano-core/src/engine.rs`): Listen; Learn (LEDs show the
  next chord, song waits); **Follow** (adaptive: plays like Listen; a hand the
  learner hits goes soft at 40 % velocity as a guide, back to full after
  `FOLLOW_IDLE_MS` = 4 s without hits; only active hands are scored).
* **Piano-key remote** (`piano-core/src/controls.rs`, every board + Android):
  A0/B0 song, A#0/C#1 tempo, C1 play/pause, D1 restart, E1/F1 category
  (random song from a top-level folder), G1 random, G#1 shuffle-play
  folder, D7/E7/F7 hands, G7/A7/B7 mode, C8 bookmark. A control key
  works when nothing plays; mid-song it is a note if the song plays it
  (learner's hands) within −2…+3 s (`controls::note_near`; the log says
  "control key F1 is a note here"). Every board boots in Follow with a random 儿时回忆 song
  that starts when the piano connects.

## Conventions the user expects

* **Commit every change** with a detailed message (what, why, how verified),
  ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Don't push unless asked. The Android app is its own repo (commit there
  separately).
* Keep the docs in step: `README.md` (feature rows, board table),
  `doc/piano_coach_firmware.md` (relevant section, architecture bullet,
  "Verified on hardware" paragraph), `boards/README.md` for contract or
  porting changes. Code comments explain *why*; match the existing style.
* Ports and features are verified on real hardware with the user at the
  piano: describe **one test per question**, give a long lead-in, keep LEDs
  lit 15–20 s for LED probes, and ask a single multiple-choice question.
  Don't claim something works on a board that wasn't tried — say so.
* When something must be decided by the user (key layout, behaviour), say
  what you recommend and why; when it's a reasonable default, just do it and
  mention it.

## State (2026-10-16) and open ends

* Original-ESP32 BLE drops fixed (host flow control off; verified on the
  CYD, Audio Kit and WROVER-KIT). An nRF52840 dongle (Nordic
  Open DFU bootloader) is on USB too — could become a BLE sniffer.
* Latest firmware commits: YD-ESP32-S3 port (aed283e), adaptive Follow
  (37abc72), piano-key remote + boot song (12625bf), shuffle-play (230d3be).
  Android app `../piano-coach` 9e8a8b5 has the same Follow/remote/shuffle.
* Verified on hardware: everything on the Tab5; YD-ESP32-S3 (USB+BLE switch,
  BOOT key, web page); Audio Kit (speaker, keys, web page); the small boards
  and WROVER-KIT up to the earlier features. The remote (every key, incl.
  E1/F1 by category and shuffle on/skip/off) verified on the 1732S019
  (2026-10-16), E1/F1 by category also on the Tab5 (microSD) and the F888
  app; song keys used on the CYD, Audio Kit and WROVER-KIT. The
  Korvo has the latest firmware but had no microSD card in at the last
  boot.
* Not done / ideas: bookmarks on small and headless boards (C8 does nothing
  there); search and A–B loop on small boards; sheet view on small boards;
  the synth is additive, not sampled; see "Known limits / ideas" in
  `doc/piano_coach_firmware.md`.
