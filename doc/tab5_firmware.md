# Piano Coach — M5Stack Tab5 firmware

Rust firmware for the M5Stack Tab5 (ESP32-P4) that plays MIDI songs from
the microSD card on the GZUT 88-key LED piano and teaches them. In
**Learn** mode the piano's red key LEDs show the next chord, and the song
waits until you play it. The piano connects over **USB** (Tab5 USB-A port)
or **Bluetooth LE** (via the Tab5's ESP32-C6), selectable in Settings.

Sources: `firmware/tab5` (device crate) and `firmware/piano-core`
(board-independent logic with host tests). Piano protocol:
[GZUT_piano_protocol.md](GZUT_piano_protocol.md).

## Using it

1. Copy the song library to a FAT32/exFAT microSD card. A top-level
   `midi/` folder is used if present (else the card root); sub-folders
   are browsable. `.mid`, `.midi`, `.kar`, `.rmi` are listed.
2. Connect the piano: USB-C cable from the piano to the Tab5's **USB-A**
   port, or choose **Settings → Connect via → Bluetooth** (the Tab5
   restarts and connects to `GZUT-MIDI` automatically). The top-right chip
   turns green when connected.
3. **Library**: tap folders to browse, tap a song to load it (duration,
   tempo, notes per hand are shown), then **Learn**, **Follow** or
   **Listen**. **Search** (top right of the list) finds a song anywhere on
   the card from the full-width on-screen letter keyboard (~80×66 px keys; the
   song card narrows while searching): type **pinyin initials**
   (`mbg` → 卖报歌), **full pinyin** (`xiaoxingxing` → 小星星) or Latin
   text (`once more`); titles and artists are matched, best matches
   first, up to 200 results.
4. **Play** page: mode (Learn / Follow / Listen), hands (Both / Right /
   Left), restart / play-pause / stop, tap the progress bar to seek,
   tempo −/+ (30–200 %). **Practice a section**: tap **A** where the
   section starts (it snaps back to the chord at or before that moment)
   and **B** where it ends; playback then jumps back to A at B in every
   mode — in Learn the section's first chord lights up again — and the
   pass counter (#2, #3…) shows top-right. The ✕ button clears the loop;
   loading another song clears it too. Falling notes show what is coming (teal right hand,
   violet left hand, red = what to play now); the on-screen keyboard
   mirrors the piano's LEDs (red), your key presses (blue) and wrong
   notes (amber).

| Mode | What happens |
|---|---|
| **Learn** | The next chord lights up silently (Note On velocity 1). The song waits until every lit key is played; the LED goes out as each key is hit. With "Piano plays the other hand" on, the other hand is played for you in step. Accuracy, chord count and streak are shown. |
| **Follow** | The song runs at the chosen tempo; your hand's keys light up ahead of time ("light keys ahead", 200–1500 ms), and hits within −250/+300 ms are scored. |
| **Listen** | The piano plays the song (LEDs follow the notes). The hands selector picks what is played. |

Settings (all saved in NVS): Bluetooth/USB, accompaniment, easy mode (any
key advances — for children), hint LED off when pressed, follow lead
time, brightness, **Light all keys** / **All off** test buttons, the
piano's last panel change (volume/tone from its vendor SysEx), and SD
**Rescan** (also mounts a card inserted after boot).

Hands are assigned from track names (left/right/左/右), else the
higher-pitched of two tracks is the right hand, else notes split at middle
C. In the bundled JustPiano library 2200 of 2449 songs have exactly two
note tracks.

## Building and flashing

Prerequisites: Rust nightly with `rust-src` (pinned by
`rust-toolchain.toml`), `cargo install ldproxy espflash`, and ESP-IDF
v5.5.3 with its toolchain. `.cargo/config.toml` points
`ESP_IDF_TOOLS_INSTALL_DIR` at the existing herdline checkout
(`~/w/rust/herdline/clients/tab5/.embuild/espressif`); set
`ESP_IDF_TOOLS_INSTALL_DIR=workspace` to let embuild download its own.

```bash
cd firmware/tab5
cargo build --release
cargo run --release -- --port /dev/ttyACM0
```

`cargo run` uses `flash.sh`, which flashes the ELF **with this build's
bootloader and partition table** (espflash's bundled P4 bootloader does
not boot rev-1.x silicon). Without the monitor:

```bash
espflash flash --port /dev/ttyACM0 --bootloader target/riscv32imafc-esp-espidf/release/build/bootloader.bin --partition-table target/riscv32imafc-esp-espidf/release/build/partition-table.bin target/riscv32imafc-esp-espidf/release/piano-tab5
```

Do not use `espflash monitor --before no-reset`: it leaves the chip in
the ROM bootloader.

Host tests (SMF parser, engine incl. loops, codecs, search; also parses and indexes the whole library at
`~/w/piano/midi-piano/assets/midi` or `$PIANO_MIDI_DIR`):

```bash
cd firmware/piano-core
cargo +stable test --release
```

Fonts: `tools/make_font.py` (needs `pip install fonttools`) regenerates
`ui/fonts/NotoSansSC-Piano.otf` (Noto Sans CJK SC subset: ASCII,
Latin-1, punctuation, the 3755 GB2312 level-1 hanzi and every character
in the library's names) and `ui/glyphs.slint`, and the Font Awesome icon
subset. Slint's software renderer can only draw glyphs embedded at build
time, so re-run it after adding songs with rarer characters:

```bash
tools/make_font.py --library /path/to/midi
```

## Architecture

```
main.rs ── app task: board bring-up, NVS, transport choice, SD mount
  ├─ usb_midi.rs  (USB mode)  USB-host class client  ─┐  KeyEvent ▶
  ├─ ble_midi.rs  (BLE mode)  NimBLE central via C6  ─┤  ◀ MIDI msgs
  ├─ player.rs    engine thread, 1 kHz, owns piano_core::Engine
  └─ ui.rs        Slint software renderer on the DPI framebuffer + touch
piano-core: smf · song · engine · usbmidi · blemidi · library · search
```

* **Board** (`board.rs`, BSP in `components/m5stack_tab5_noglib`, vendored
  from herdline): I2C bus recovery, both PI4IOE5V6408 expanders
  programmed explicitly, display power-cycle, MIPI-DSI panel + touch.
  The BSP probes the touch controller: ST7123 integrated
  display+touch (current boards) or ILI9881C + GT911 (early ones).
  Landscape UI rendered with 270° rotation into the portrait framebuffer,
  partial repaint into a PSRAM shadow buffer, double-buffered scanout.
* **One transport per boot.** The USB-HS host and the C6's SDIO link
  interfere on the Tab5 (found in herdline), so USB mode powers USB-A
  5 V and leaves the C6 off, and BLE mode powers the C6 and leaves USB-A
  off. Switching saves `transport` in NVS and restarts.
  ESP-Hosted's boot constructor only prepares its transport; the C6 is
  contacted only by `esp_hosted_connect_to_slave()` in BLE mode.
* **USB MIDI**: IDF USB host library client. It parses the configuration
  descriptor for Audio/MIDI-Streaming (class 1/3) and its bulk endpoints,
  keeps one IN transfer queued, and sends OUT as USB-MIDI event packets,
  **at most 8 per transfer, 12 ms apart** under backlog (the piano drops
  events from large bursts; all 88 LEDs light with this pacing).
* **BLE MIDI**: C6 controller (needs ESP-Hosted slave firmware with BT;
  this Tab5 runs 2.12.11, the factory 0.0.0 image has none) + NimBLE host.
  It scans for `GZUT-MIDI` or the MIDI service UUID, connects, exchanges
  MTU (260), finds the characteristic, and enables notifications at
  value + 1. Every write is `80 80` + plain MIDI messages, packed up to
  MTU − 3 (max 244) bytes, write-without-response. It reconnects
  automatically. The piano accepts only one BLE central: a PC or phone
  holding the link keeps it from advertising.
* **Engine** (`piano-core/src/engine.rs`): pure state machine fed with
  the clock, key events and commands. Hint and sounding notes are
  ref-counted per key, so overlapping notes and accompaniment Note Offs
  never leave a hint dark. Everything is sent on channel 1. Program
  Change 0 is sent at play start; stop/pause send CC 64 0 + CC 123.
* **Search** (`piano-core/src/search.rs`): an index of every song on the
  card is built on a background thread at boot and on Rescan (2449 songs
  in ~4.9 s). Each title/artist gets full-pinyin and initials keys from
  `pinyin_table.rs` (generated by `tools/make_font.py` with pypinyin for
  all 3887 hanzi the font can draw); a query matches as a substring.
* **Loop** (`Engine::set_loop`): checked each tick before the Learn halt,
  so a chord past B is never waited for or lit; on reaching B the engine
  silences, seeks to A (re-lighting its first chord) and counts a pass.
  Loops shorter than 500 ms are rejected.
* `CONFIG_FREERTOS_HZ=1000` for ms-accurate timing; PSRAM XIP; 12 MB app
  partition (image ≈ 4.9 MB).

## Verified on hardware (2026-10-15)

Tab5: ESP32-P4 rev v1.3, 16 MB flash, ST7123 panel, C6 ESP-Hosted
2.12.11.

| Check | Result |
|---|---|
| Boot, display, touch, dark UI | ✔ |
| USB: piano enumerates (`CHMidi-2.3`, 5022:0716, IN 0x82 / OUT 0x02) | ✔ |
| USB: "Light all keys" — all 88 LEDs, then off | ✔ |
| microSD mount, Chinese folder/song names render | ✔ |
| BLE: C6 controller up, NimBLE sync, connect ~2.6 s after boot, handle 9, MTU 260 | ✔ |
| BLE: all 88 LEDs; Learn mode LEDs guide and key presses advance | ✔ |
| USB Learn / Follow / Listen end-to-end | reported working via the UI; not separately logged |
| Search: `mbg` → 卖报歌, `xiaoxingxing` → 小星星; result loads | ✔ |
| A–B loop: jumps back at B, re-lights first chord, pass counter | ✔ (over Bluetooth) |

## Known limits / ideas

* Search uses each hanzi's most common reading (pypinyin, without
  context), so a polyphonic character in a title may need its other
  reading's letter; characters outside the font's set have no pinyin.
* The loop end is a time, not a bar line; there is no bar/beat grid yet.
* Velocity from the piano is ignored for scoring (it is quantised to ~6
  levels anyway).
* If the C6 has factory firmware, BLE mode reports "C6 Bluetooth
  unavailable" — update it (herdline's `c6ota` feature does this over WiFi).
