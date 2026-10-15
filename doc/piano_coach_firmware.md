# Piano Coach — firmware

Rust firmware for ESP32 dev boards that plays MIDI songs from
the microSD card on the GZUT 88-key LED piano and teaches them. In
**Learn** mode the piano's red key LEDs show the next chord, and the song
waits until you play it. The piano connects over **USB** (the board's
USB-A host port) or **Bluetooth LE**, selectable in Settings.

| Board | MCU | Display | Notes |
|---|---|---|---|
| M5Stack Tab5 | ESP32-P4 | 5″ 1280×720 | BLE via its ESP32-C6; side power button |
| ESP32-S31-Korvo-1 V1.1 | ESP32-S31 (dual RISC-V 320 MHz, native Wi-Fi 6 / BLE 5.4) | 4.3″ 800×480 (SUB3), UI at 0.625 scale | no backlight control; shut down = deep sleep (RESET restarts); PLAY / SET / VOL± buttons |

Board support is a layer of its own (`firmware/piano-coach/src/board/`,
`boards/`); see [boards/README.md](../firmware/piano-coach/boards/README.md)
for how to add a board.

Sources: `firmware/piano-coach` (device crate) and `firmware/piano-core`
(board-independent logic with host tests). Piano protocol:
[GZUT_piano_protocol.md](GZUT_piano_protocol.md).

## Using it

1. Copy the song library to a FAT32/exFAT microSD card. A top-level
   `midi/` folder is used if present (else the card root); sub-folders
   are browsable. `.mid`, `.midi`, `.kar`, `.rmi` are listed.
2. Connect the piano: USB-C cable from the piano to the board's **USB-A**
   port, or choose **Settings → Connect via → Bluetooth** (the board
   restarts and connects to `GZUT-MIDI` automatically). The top-right chip
   turns green when connected.
3. **Library** — four views, chosen with the icon tabs at the top of the
   list:
   * **Folders** (browse the card), **★ Bookmarks**, **Recently played**;
   * **Song globe**: every song is a dot on a sphere, coloured by category
     (the first folder level of the library: 儿时回忆, 红色歌谣, 流行空间, 经典乐章,
     影视剧场 for the bundled one) and gold if bookmarked. Drag to spin it
     (it keeps spinning, then idles slowly), tap a dot to pick that song
     (white ring; its name shows in the header), tap a colour chip to turn
     that category towards you.

   Lists scroll by flicking or with the wide scrollbar on
   the right (drag the thumb, or tap the track to jump). Tap folders to browse, tap a song to load it (duration,
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

**Bookmarks and scores**: the ☆ on the song card bookmarks a song (★ in
lists, gold on the globe). Finishing a Learn or Follow run that covered at
least 90 % of the song records its accuracy; the card shows the best Learn
and Follow scores as stars (★ ≥ 60 %, ★★ ≥ 80 %, ★★★ ≥ 95 %) and the play
count, lists show the best stars, and a better result gets a "New best!"
toast. Everything is stored on the card in `.pianocoach/progress.tsv`
(one tab-separated line per song, readable on a PC).

**Speaker**: **Settings → Notes sound from** selects **Piano** (default),
**Speaker** or **Both**, with a speaker volume slider (Korvo: also VOL± on
the board).

**Korvo buttons**: **PLAY** = play/pause, **SET** = next page (Library →
Play → Settings → Library), **VOL+ / VOL−** = speaker volume ±10 %. With **Speaker**, played
notes (Listen, and the accompaniment in Learn/Follow) come from the board's
speaker while the piano's LEDs still follow them silently — so songs can
also be listened to with no piano connected. Your own playing always
sounds on the piano.

**Shut down**: the power icon at the top right (or **Settings → Shut
down**) asks for confirmation, stops playback and switches every piano LED
off, then (Tab5) pulses its hardware power latch (POWEROFF_GEN, expander
0x44 P4). On battery this cuts power. With USB-C power connected the latch
cannot, so the firmware turns the screen, USB-A and C6 off and enters deep
sleep. In both cases one press of the side power button starts it again. The
Korvo has no latch: shut down blanks the screen and deep-sleeps; **RESET**
restarts it.
Verified on the bench with USB-C connected (deep-sleep path); the
battery-only path (latch cutting power) is not verified yet.

Settings (all saved in NVS): Bluetooth/USB, accompaniment, easy mode (any
key advances — for children), hint LED off when pressed, follow lead
time, brightness (boards with backlight control), **Show touches** (a red dot
under the finger, for checking a touch panel), **Light all keys** / **All off** test buttons, the
piano's last panel change (volume/tone from its vendor SysEx), and SD
**Rescan** (also mounts a card inserted after boot).

Hands are assigned from track names (left/right/左/右), else the
higher-pitched of two tracks is the right hand, else notes split at middle
C. In the bundled JustPiano library 2200 of 2449 songs have exactly two
note tracks.

## Building and flashing

Prerequisites: Rust nightly with `rust-src` (pinned by
`rust-toolchain.toml`), `cargo install ldproxy espflash`, ESP-IDF v5.5.3
(Tab5; embuild fetches it) and an ESP-IDF **master** checkout for the
ESP32-S31 (default `/home/nws/w/esp32/esp-idf`, override with `IDF_PATH`).
Tools are shared with the herdline project's install
(`~/w/rust/herdline/clients/tab5/.embuild/espressif`); set
`ESP_IDF_TOOLS_INSTALL_DIR=workspace` to let embuild download its own.
The esp-rs crates are source-pinned copies with ESP32-S31 support
(`firmware/piano-coach/vendor`, from herdline).

```bash
cd firmware/piano-coach
tools/board.sh tab5  build --release
tools/board.sh tab5  run   --release -- --port /dev/ttyACM0
tools/board.sh korvo build --release
tools/board.sh korvo run   --release -- --port /dev/ttyUSB3
```

`run` flashes the ELF **with the same build's bootloader and partition
table** (`flash.sh`): espflash on the Tab5 (its bundled P4 bootloader does
not boot rev-1.x silicon), `tools/flash-idf.py` on the S31: espflash cannot package S31
PSRAM-XIP images, so esptool's `elf2image` makes the image and
`espflash write-bin` writes bootloader, partition table and app (its
flasher stub: ~1.5 min for the app; esptool without a stub takes ~7 min,
`--writer esptool` forces it). Add `--no-monitor` to skip the monitor.
Each board builds into `target/<board>` with its own `sdkconfig.<board>`.

Do not use `espflash monitor --before no-reset`: it leaves the chip in
the ROM bootloader.

Host tests (SMF parser, engine incl. loops, codecs, search, synth, globe; also parses and indexes the whole library at
`~/w/piano/midi-piano/assets/midi` or `$PIANO_MIDI_DIR`):

```bash
cd firmware/piano-core
cargo +stable test --release
```

UI preview on the PC: renders every page (Library, Search, Play,
Settings, shut-down dialog) with the device's own Slint code, renderer and
embedded fonts and writes PNGs to `/tmp/piano-ui-preview` (or
`$UI_PREVIEW_DIR`). Check layout changes here before flashing:

```bash
cd firmware/piano-coach/tests/ui_preview
cargo +stable run --release --target x86_64-unknown-linux-gnu
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
  ├─ audio.rs     ES8388 + NS4150B speaker, renders piano_core::synth
piano-core: smf · song · engine · usbmidi · blemidi · library · search ·
            progress · synth · sphere
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
* **Speaker** (`audio.rs`, `piano-core/src/synth.rs`): additive piano
  voice — 6 slightly inharmonic partials with per-partial exponential
  decay (bass rings longer, treble shorter, soft notes duller), a short
  hammer-noise transient, damper on release, sustain pedal, 20 voices with
  quietest-voice stealing, soft clipping. 24 kHz mono through the BSP's
  esp_codec_dev on its own task (core 1, priority 12, above the UI); the
  codec and amp open only while voices sound. Phases are 32-bit integers
  and sine is a polynomial (no table — heap is in PSRAM): a dense song
  costs 18–32 % of one core (the first table-based version needed 146 %).
  The player routes each engine message (`player::route`): hints always to
  the piano; audible notes to the piano, the synth, or both — with the
  speaker selected the piano gets a velocity-1 copy so its LEDs follow.
* **Song globe** (`globe.rs`, `piano-core/src/sphere.rs`): Fibonacci-sphere
  layout in (category, title) order, perspective + depth shading,
  inertia and idle spin, pick-by-tap. It is painted **directly into the
  RGB565 shadow framebuffer** after Slint renders (the Slint page only
  leaves an empty panel that reports its position): handing Slint a new
  760×456 image per frame cost ~350 ms. Each frame erases last frame's
  dots from a cached background, then stamps anti-aliased dot sprites with
  integer blending; Slint is made to repaint fully when the globe is hidden
  or moves, so no stale dots survive. ~10 fps while spinning.
* Release builds use `opt-level = 3` for piano-core, the firmware and
  Slint's software renderer (`"s"` elsewhere): the globe's projection/sort
  went from 15 to 4.5 ms. Image ≈ 7.6 MB of the 12 MB partition.
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
| Shut down → LEDs off, Tab5 off (USB-C powered: deep-sleep fallback); power button boots | ✔ |
| Tab5 speaker playback ("sounds OK", no stutter on a dense song; 18–32 % CPU) | ✔ |
| Bookmarks listed under ★; Learn score recorded with "New best!"; survives reboot | ✔ |
| Song globe: spin, tap-to-pick, category chips; no leftovers on screen switches | ✔ |

Korvo: ESP32-S31-Korvo-1 V1.1 (ESP32-S31-WROOM-3, 16 MB flash, 16 MB
PSRAM), SUB3 800×480 RGB panel with a GT1158 touch controller, ESP-IDF
master v6.1-dev (4d59230ddff).

| Check | Result |
|---|---|
| Boot, display at 0.625 scale, dark UI ("looks right") | ✔ |
| microSD mount, library indexed (2449 songs) | ✔ |
| USB-A (HS host): piano enumerates (`CHMidi-2.3`), "Light all keys", Learn LEDs | ✔ |
| Touch | ✔ except a full-height dead strip at ≈ 50–62 % of the width (no points reported). The independent `korvo-radio` bare-metal demo shows the same strip, so it is this panel/controller, not the firmware. SET reaches every page meanwhile |
| ES8389 codec initialised (48 kHz stereo); four ADC keys created | ✔ (sound and key presses not yet checked) |
| BLE, speaker playback, PLAY / SET / VOL± keys, song globe | not yet verified |

## Known limits / ideas

* Search uses each hanzi's most common reading (pypinyin, without
  context), so a polyphonic character in a title may need its other
  reading's letter; characters outside the font's set have no pinyin.
* The loop end is a time, not a bar line; there is no bar/beat grid yet.
* The speaker synth is a simple additive model, not sampled piano; it
  uses a piano voice for every track (no General MIDI instruments).
* Globe: ~10 fps while spinning (PSRAM bandwidth is shared with the
  display's scan-out); scores are per song and mode, not per hand.
* Velocity from the piano is ignored for scoring (it is quantised to ~6
  levels anyway).
* Korvo: the top-bar Settings tab sits in the bench unit's dead touch
  strip; use SET to get there (or a panel without the defect).
* If the C6 has factory firmware, BLE mode reports "C6 Bluetooth
  unavailable" — update it (herdline's `c6ota` feature does this over WiFi).
