# Piano Coach — firmware

Rust firmware for ESP32 dev boards that plays MIDI songs on the GZUT
88-key LED piano and teaches them. In
**Learn** mode the piano's red key LEDs show the next chord, and the song
waits until you play it. The piano connects over **USB** (the board's
USB-A host port) or **Bluetooth LE**, selectable in Settings. The small
boards have a minimal UI, keep their songs in flash and use Bluetooth
([Small boards](#small-boards-minimal-ui)).

| Board | MCU | Display | Notes |
|---|---|---|---|
| M5Stack Tab5 | ESP32-P4 | 5″ 1280×720 | BLE via its ESP32-C6; side power button |
| ESP32-S31-Korvo-1 V1.1 | ESP32-S31 (dual RISC-V 320 MHz, native Wi-Fi 6 / BLE 5.4) | 4.3″ 800×480 (SUB3), UI at 0.625 scale | no backlight control; shut down = deep sleep (RESET restarts); PLAY / SET / VOL± buttons |
| ESP32-1732S019 | ESP32-S3 (Xtensa, 16 MB flash, 8 MB PSRAM) | 1.9″ 170×320 ST7789, used as 320×170, no touch | minimal UI; BLE only; all 2449 songs in flash; BOOT key |
| ESP32-2424S012C | ESP32-C3 (RISC-V 160 MHz, 400 KB RAM, 4 MB flash) | 1.28″ round 240×240 GC9A01 + CST816D touch | minimal UI; BLE only; 776 songs (≤ 1200 notes) in flash; BOOT key |
| LILYGO T-Display-S3 | ESP32-S3R8 (16 MB flash, 8 MB PSRAM) | 1.9″ 170×320 ST7789 on an 8-bit parallel bus, used as 320×170; touch on the Touch variant | minimal UI; BLE only; all 2449 songs in flash; BOOT + KEY keys |

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
   **Keys / Sheet** (right of the hands switch, remembered) switches to
   **full-screen sheet music**: the top bar and controls give way to a
   slim band (song, Learn prompt, score, restart, play/pause, tempo −/+,
   colour scheme, note names, ✕ back to the keyboard view) and the rest of
   the screen is a grand staff (right hand treble, left hand bass) as
   large as two lines fit — a 16 px staff space on the Tab5 — with clefs,
   key and time signatures, beamed eighths/sixteenths, rests, ties and
   accidentals. A blue cursor follows the music and the page turns when
   it reaches the lower line.
   **Colour notation** (band button or Settings → Sheet music): note heads
   coloured by pitch class — **Boomwhackers** (default: C red, D orange,
   E yellow, F lime, G green, A purple, B magenta, sharps/flats between
   their neighbours), **Rainbow** (Chroma-Notes style: C red … G blue,
   A indigo, B violet) or **Ink** (black). Filled heads keep a dark rim so
   light colours read on paper. The notes to play now get a red halo (in
   Ink, they turn red). **Note names** in the heads: letters (C D E) or
   fixed-do solfège (Do Re Mi), spelled by the key signature.

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

## Small boards (minimal UI)

The ESP32-1732S019 (1.9″ bar), the LILYGO T-Display-S3 (1.9″ bar,
two keys) and the ESP32-2424S012C (1.28″ round, watch-like) have no
microSD slot, USB host or speaker. Piano Coach keeps
a **song pack** in their flash, reaches the piano over **Bluetooth**
only (it connects to `GZUT-MIDI` by itself; the dot top-left/top-centre
turns green) and has a minimal UI with the essentials:

* **Library** — a wheel of folders and songs (the focused row in the
  middle, its neighbours above and below, "3 / 41" at the bottom);
  songs show their best stars. The first row is **Settings** at the top
  level and **Back** in folders.
* **Song** — **Learn**, **Follow**, **Listen** (with best score),
  **Hands** (Both / Right / Left) and **Tempo** (50–120 %), then Back.
* **Play** — the next keys to play in large letters (red, like the
  LEDs), "Play the lit keys" while Learn waits, ✓ / ✕ counts, time and
  progress (a bar, or a ring of dots around the round screen). At the end:
  stars, accuracy and "New best!". Best Learn / Follow scores per song
  are kept in NVS.
* **Settings** — easy mode, hint off on hit, accompaniment, follow lead
  time, brightness, **Light all keys**, **All LEDs off**, board/version/
  song count, **Sleep** (deep sleep; BOOT wakes the 1.9″ board, a touch
  wakes the round one, RESET wakes both).

Controls:

| | Menus | Playing |
|---|---|---|
| BOOT tap | next row | pause / resume (restart when finished) |
| BOOT double tap | previous row | restart |
| BOOT hold (0.6 s) | open / OK | back to the song menu |
| T-Display-S3 KEY / BOOT | KEY: next row (double: 5 rows), hold OK; BOOT: previous row (double: 5 rows), hold back | KEY: pause / resume; hold BOOT: back |
| Touch (round board, T-Display-S3 Touch) | tap a row; swipe up/down to scroll; swipe right = back | tap = pause / resume; swipe right = back |
| Piano keys (while nothing plays) | **A0** previous, **B0** next, **C1** OK, **D1** back | when paused/finished: C1 resume, D1 back, A0 restart |

The four lowest white keys steer the menus only while no song is playing
or waiting for you, so they never interfere with a lesson.

**Songs**: `tools/songs.sh <board> build` packs the library (default
`~/w/piano/midi-piano/assets/midi`) into `target/<board>/songs.bin` with
the board's `SONGPACK_ARGS`, and writes `boards/<board>/glyphs.slint` —
the characters of the packed names, which the firmware embeds (rebuild
after it changes). `tools/songs.sh <board> flash --port …` writes the
pack to the `songs` partition; the firmware is flashed separately, and
neither overwrites the other. The 1.9″ board takes the whole library
(8.9 MB in 11.9 MB); the round board takes 1.4 MB: the children's,
revolutionary and film categories first, then others, only songs with
≤ 1200 notes (a song's engine data must fit beside Bluetooth and the
UI in the C3's ~150 KB of free heap).

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

Small boards (Xtensa needs the esp-rs toolchain: `espup install --name
esp198 --toolchain-version 1.98.1.0 --targets esp32s3 --std`, Slint needs
Rust ≥ 1.92; `tools/board.sh` picks it and its libclang):

```bash
tools/songs.sh 1732s019 build && tools/songs.sh 1732s019 flash --port /dev/ttyUSB0 --baud 921600
tools/board.sh 1732s019 run --release -- --port /dev/ttyUSB0
tools/songs.sh 2424s012 build && tools/songs.sh 2424s012 flash --port /dev/ttyACM0
tools/board.sh 2424s012 run --release -- --port /dev/ttyACM0
tools/songs.sh tdisplays3 build && tools/songs.sh tdisplays3 flash --port /dev/ttyACM0
tools/board.sh tdisplays3 run --release -- --port /dev/ttyACM0
```

A T-Display-S3 running firmware that takes over its USB port (e.g. a
"BLE USB Dongle" image: the PC sees a Bluetooth adapter, no serial port)
must be put in download mode once: hold BOOT, press RST, release BOOT —
`/dev/ttyACM*` appears. Piano Coach keeps the USB Serial/JTAG console, so
later flashes need no buttons.

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

Host tests (SMF parser, engine incl. loops, codecs, search, synth, globe,
song packs; also parses and indexes the whole library at
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

The minimal UI has its own preview (bar and round screens, menu / song /
settings / play / result → `mini-*.png`):

```bash
cd firmware/piano-coach/tests/mini_preview
cargo +stable run --release --target x86_64-unknown-linux-gnu
```

`piano-core/examples/memprobe.rs` prints the heap a song needs (parse
peak, song, engine) — the numbers behind the round board's note limit.

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
* **Sheet music** (`piano-core/src/notation.rs`, `ui/sheet.rs`): the
  parser keeps time and key signatures and the tempo map in quarter
  notes (`song::Meter`; a file without a key signature gets the key whose
  scale needs the fewest accidentals). Engraving snaps onsets and lengths
  to 16ths (86 % of the library's onsets are on that grid), builds
  measures from the time signatures (lengths snapped to the grid for
  x/32 meters), one voice per staff (a chord lasts until the staff's next
  onset), splits notes at barlines into tied values, fills gaps with
  rests, spells black keys by the key signature with accidentals carried
  through the measure, beams flagged notes per beat, spaces onsets by
  duration and justifies each line of measures. A note far outside its
  hand's staff (below G3 / above F4) moves to the other staff. Output is
  primitives — rectangles, glyphs, dots, flags, ties, digits — that the
  Play page draws from six Slint models, two systems at a time; heads
  carry their note's start so Slint colours the current ones red. Every
  library song engraves (host test), ≤ 62 ms on the PC; on the Tab5 卖报歌
  takes 22 ms and a 24-line score 149 ms. The staff space follows the
  screen (two systems of 20.5 spaces in the height below the band); the
  colour and name options only refill the models. The music symbols are Noto Music glyphs exported as
  SVGs (`tools/make_music_glyphs.py`) rather than an imported font:
  slint-build gives every font the UI's whole character set (filling gaps
  from system fallback fonts), which re-embedded the ~4000 CJK glyphs
  (+2 MB).
* **Board classes** (`boards/<board>/board.env` `UI=` / `HAS=`, turned
  into `cfg(ui_full)` / `cfg(ui_mini)` / `cfg(has_usb|has_sd|has_speaker)`
  by build.rs): the full UI (`ui.rs`, `ui/app.slint`) on framebuffer
  boards; the minimal UI (`ui_mini.rs`, `ui/mini/app.slint`) on small SPI
  panels. `board/contract.rs` checks the common items plus the class's
  own (framebuffer, audio, USB, SD — or `lcd_init` / `lcd_draw` /
  `touch_point`).
* **Minimal UI rendering**: no framebuffer (the C3 cannot spare
  115 KB): Slint's `render_by_line` renders only the dirty region, lines
  with the same x-range are gathered into a 10-line DMA band, byte-swapped
  to big-endian RGB565 and sent with `esp_lcd_panel_draw_bitmap`; the
  call waits on the transfer-done interrupt (`board/spi_lcd.rs`). The
  menus are Rust-side lists; Slint only gets the three visible rows. The
  BOOT key is debounced and decoded into tap / double tap / hold
  (`board/key.rs`); the CST816D is read over I2C each frame and swipes are
  classified in `ui_mini.rs` (taps are passed to Slint's TouchAreas).
* **Song pack** (`piano-core/src/songpack.rs`, `songs.rs`): a 32-byte
  header, a 28-byte-per-entry tree index (folders' children contiguous in
  library order), names, then zlib-compressed MIDI files. The firmware
  memory-maps the `songs` partition (no RAM for the index), validates
  the index CRC, and inflates one song on load into an exact-size buffer
  with a heap-allocated decompressor (miniz_oxide's helpers build its
  11 KB state — the stream API also a 32 KB window — on the stack first).
  Pack songs are virtual paths `/pack/<index>`, so the player loads them
  like files.
* **Lean parsing** (all boards): the parser keeps only the currently held
  notes per track (it had 2048 per-key queues — 24 KB per track on a
  32-bit MCU) and track names stay off the event list; a 300-note song
  went from a 99 KB to a 34 KB peak.
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
| Sheet music: readable grand staff, red current notes in Learn, cursor, smooth page turns in Listen | ✔ |
| Full-screen sheet, Boomwhackers / Rainbow colours, letter / solfège names, band controls | ✔ |

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

1.9″ ESP32-1732S019 (ESP32-S3 rev 0.2, 16 MB flash, 8 MB PSRAM) and
1.28″ ESP32-2424S012C (ESP32-C3 rev 0.4, 4 MB flash, CST816D touch):

| Check | 1.9″ (S3) | 1.28″ round (C3) |
|---|---|---|
| Boot, display orientation and colours | ✔ | ✔ (after un-mirroring X) |
| Song pack mounted | ✔ 2449 songs, 8.9 MB | ✔ 776 songs, 1.33 MB |
| BLE: connects to GZUT-MIDI by itself | ✔ | ✔ (~2.5 s after boot) |
| Learn: LEDs light, playing advances | ✔ | ✔ |
| BOOT tap / double tap / hold | ✔ | ✔ |
| Touch: tap rows, swipe scroll, swipe-right back | — | ✔ |
| Piano keys A0/B0/C1/D1 as menu keys | ✔ | not separately tried (same code) |
| Heap | 8.3 MB free (PSRAM) | 155 KB idle; 88 KB with a 761-note song, 40 KB low-water |
| App image | 2.9 MB of 4 MB | 2.3 MB of 2.5 MB (Slint built for size) |

LILYGO T-Display-S3 (ESP32-S3R8 rev 0.2, no touch): display (8-bit
parallel ST7789, vendor init sequence) right way up and in colour, 2449
songs mounted, BLE connects, KEY / BOOT navigation and Learn ✔; app
2.9 MB of 4 MB, 8.3 MB heap free. The touch variant's CST816 is probed
at boot; its landscape mapping is untested.

## Known limits / ideas

* Search uses each hanzi's most common reading (pypinyin, without
  context), so a polyphonic character in a title may need its other
  reading's letter; characters outside the font's set have no pinyin.
* The loop end is a time, not a bar line (the sheet knows the bars now;
  snapping A/B to them would be a next step).
* Sheet music is practical, not engraved: 16th grid (triplets show as
  the nearest 16ths), one voice per staff, horizontal beams, no
  dynamics/pedal/fingering, no 8va lines (very high or low notes get many
  ledger lines). Small boards have no sheet view.
* The speaker synth is a simple additive model, not sampled piano; it
  uses a piano voice for every track (no General MIDI instruments).
* Globe: ~10 fps while spinning (PSRAM bandwidth is shared with the
  display's scan-out); scores are per song and mode, not per hand.
* Velocity from the piano is ignored for scoring (it is quantised to ~6
  levels anyway).
* Korvo: the top-bar Settings tab sits in the bench unit's dead touch
  strip; use SET to get there (or a panel without the defect).
* Small boards: no search, bookmarks, A–B loop, speaker or song globe;
  the round board's pack holds songs up to 1200 notes and loading one
  takes ~1.4 s (the C3 has no FPU). Long folders are browsed a row at a
  time (swipes move several rows).
* If the C6 has factory firmware, BLE mode reports "C6 Bluetooth
  unavailable" — update it (herdline's `c6ota` feature does this over WiFi).
