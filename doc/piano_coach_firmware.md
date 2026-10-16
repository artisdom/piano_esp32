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
| ESP-WROVER-KIT v4.1 | ESP32 rev 1 (dual Xtensa 240 MHz, 4 MB flash; PSRAM unused) | 3.2″ 320×240 ILI9341 / ST7789V (auto-detected), UI at 1.25× | minimal UI; BLE only; 776 songs (≤ 1200 notes) in flash; BOOT key; **remove any microSD card** |
| Ai-Thinker ESP32-Audio-Kit | ESP32 rev 3 (4 MB flash, 4 MB PSRAM used), ES8388 codec | none — keys, tones and a Wi-Fi web page | headless; BLE only; whole library from microSD; speaker synth; six keys |
| LILYGO T-Display-S3 | ESP32-S3R8 (16 MB flash, 8 MB PSRAM) | 1.9″ 170×320 ST7789 on an 8-bit parallel bus, used as 320×170; touch on the Touch variant | minimal UI; BLE only; all 2449 songs in flash; BOOT + KEY keys |
| VCC-GND YD-ESP32-S3 | ESP32-S3-WROOM-1 N16R8 (16 MB flash, 8 MB PSRAM) | none — BOOT key, RGB LED and a Wi-Fi web page | headless; **USB-C OTG and BLE at once** (USB preferred); all 2449 songs in flash |

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
| **Follow** | The song runs at the chosen tempo and plays like Listen, with the tracked hands' keys lit ahead of time ("light keys ahead", 200–1500 ms). Join in with a hand and, from your first hit on one of its notes, the piano plays that hand softly (40 % velocity) as a guide while the other stays at full volume; play both and both go soft. A note you play early is not struck again. After 4 s of playing time without a hit on a hand, it is loud again. Only hands you are playing are scored (hits within −250/+300 ms; misses and wrong keys count only meanwhile). Hands = Both tracks both hands; Right or Left tracks only that one, and the other always plays normally. |
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
notes (Listen, and what the piano plays in Learn/Follow) come from the board's
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

## The piano as a remote control (every board)

Every board **starts in Follow** with a **random song from 儿时回忆**,
which begins as soon as the piano is connected (at once when the board's
speaker plays the notes). From then on the piano's lowest and highest keys
work as buttons — no screen or phone needed:

| Low end (left hand) | | High end (right hand) | |
|---|---|---|---|
| **A0** | previous song in the folder | **D7** | Left hand |
| **A#0** | tempo −10 % | **E7** | Both hands |
| **B0** | next song in the folder | **F7** | Right hand |
| **C1** | play / pause | **G7** | Learn |
| **C#1** | tempo +10 % | **A7** | Follow |
| **D1** | restart the song | **B7** | Listen |
| **E1** | previous category, random song | **C8** | bookmark the song (Tab5 / Korvo) |
| **F1** | next category, random song | | |
| **G1** | random song in this folder | | |
| **G#1** | shuffle-play this folder (on / off) | | |

A control key works whenever nothing is playing. During a song it is a
note instead when the song plays that key, in a hand you play, from 2 s
before to 3 s after where the song is (song time; Follow lights notes up
to 1.5 s ahead) — so a real note is never taken for a command, but a key
the song only uses elsewhere still works. (Checking the whole song, as
before, left F1 dead in 15 % of the 2449 songs and G1 in 24 %.) When a
control key is taken as a note, the serial log says so ("control key F1
is a note here (−0.3 s)"); pause first (C1) and it works. The pressed key's LED blinks once
and the screen, if any, shows what it did; song changes open the Play
page (small boards: the play screen). "Categories" are the library's
top-level folders in library order (儿时回忆 → 影视剧场 → 流行空间 →
红色歌谣 → 经典乐章 on the full library), wrapping around at the end; E1 /
F1 pick a song from anywhere in the category, every song equally likely.
(They used to step the folders that hold songs, but below the categories
those are often one film or composer with a single song, so it felt like
stepping songs.) The map is `piano_core::controls`.

**Shuffle-play** (G#1, or the phone page's 🔀 button on headless boards)
plays every song of the current folder once, in random order, one after
another — the song playing when you switch it on comes first, and each
next one starts about 2.5 s after the last ends (in Learn, when you finish
it). While it is on, B0 / A0 skip forward / back in that order, and E1 /
F1 pick a random song of the next / previous category and shuffle its
folder. It ends when every song has
played, on G#1 again, or when you pick a song any other way (G1, the
screen, the phone page). Toasts show "Shuffle 3 / 41"; the phone page
shows it next to the play state.

## Small boards (minimal UI)

The ESP32-1732S019 (1.9″ bar), the LILYGO T-Display-S3 (1.9″ bar,
two keys), the ESP-WROVER-KIT (3.2″), the ESP32-2432S028R (2.8″,
resistive touch) and the ESP32-2424S012C (1.28″ round, watch-like) have no usable microSD slot, USB host or speaker. Piano Coach keeps
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
  wakes the round one, RESET wakes both). The round board also has
  **Mirror display**: its GC9A01 panels differ in scan direction between
  units (one bench unit is right as is, a second shows everything mirrored
  left-right); the setting is saved on the unit. A unit too mirrored to
  navigate can get it from the PC — see AGENTS.md ("set a pref from the
  PC").

Controls:

| | Menus | Playing |
|---|---|---|
| BOOT tap | next row | pause / resume (restart when finished) |
| BOOT double tap | previous row | restart |
| BOOT hold (0.6 s) | open / OK | back to the song menu |
| T-Display-S3 KEY / BOOT | KEY: next row (double: 5 rows), hold OK; BOOT: previous row (double: 5 rows), hold back | KEY: pause / resume; hold BOOT: back |
| Touch (round board, T-Display-S3 Touch) | tap a row; swipe up/down to scroll; swipe right = back | tap = pause / resume; swipe right = back |
| Piano keys | the remote control above (songs, folders, mode, hands, tempo): a new song opens the play screen | the same |

The small boards boot straight into the play screen with the 儿时回忆
song. Bookmarks are kept only on the Tab5 and Korvo.

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

## Boards without a display (ESP32-Audio-Kit, YD-ESP32-S3)

The ESP32-Audio-Kit and the YD-ESP32-S3 have no screen. Piano Coach runs
**headless** on them, controlled with the board's keys and a **web page on
your phone**:

* **ESP32-Audio-Kit**: the song library comes from its microSD card (as
  on the Tab5), the piano from Bluetooth; **six keys**; its speaker plays
  the built-in piano synth and short feedback tones.
* **YD-ESP32-S3**: all 2449 songs in flash (song pack, as on the small
  boards); the piano on the **USB-C OTG port** and over **Bluetooth at the
  same time** — USB is used whenever the cable is in, Bluetooth otherwise,
  switching over mid-song when the cable is plugged or pulled; the **BOOT
  key** and the **RGB LED**. No speaker, so the page has no sound-source or
  volume rows. The OTG port supplies the piano's VBUS only with the
  board's "USB-OTG" solder jumper bridged; power the board from its other
  ("COM") port.

**Web page**: the board opens a Wi-Fi network **PianoCoach-XXXX** (password
`pianocoach`). Join it with a phone; the page opens by itself ("Sign in to
network" — the board answers every DNS name and redirects the phones'
connectivity checks to itself), or open **http://192.168.4.1**. It shows the
current song, the Learn prompt, progress and score, with buttons for
previous / restart / play-pause / stop / next, Learn / Follow / Listen,
Both / Right / Left, tempo, where notes sound (piano / speaker / both) and
speaker volume; below, the library by folder and a search box (pinyin
initials such as `xxx` → 小星星, full pinyin or text). Tapping a song makes
its folder the playlist and starts it. Light all keys / All off test the
piano's LEDs.

| Key (ADF name) | Press | Hold |
|---|---|---|
| KEY1 (REC) | restart the song | notes from piano → speaker → both (1–3 beeps) |
| KEY2 (MODE) | Learn → Follow → Listen (1, 2, 3 rising beeps) | hands Both → Right → Left (octave / high / low beep) |
| KEY3 (SET) | next song in the folder (starts at once) | previous song |
| KEY4 (PLAY) | play / pause | stop |
| KEY5 (VOL+) | speaker louder | tempo +10 % |
| KEY6 (VOL−) | speaker softer | tempo −10 % |
| piano keys | the remote control (see [The piano as a remote control](#the-piano-as-a-remote-control-every-board)) | |

The Audio Kit's green LED is on while the piano is connected and blinks
while the board searches for it. The last song played is remembered.

| YD-ESP32-S3 BOOT key | Action |
|---|---|
| tap | play / pause |
| double tap | next song in the folder |
| hold (~1 s) | Learn → Follow → Listen (LED flashes white / yellow / violet) |
| piano keys | the remote control (see [The piano as a remote control](#the-piano-as-a-remote-control-every-board)) |

Its RGB LED is green while the piano is on USB, blue on Bluetooth, and
blinks amber while it searches.

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
tools/songs.sh wroverkit build && tools/songs.sh wroverkit flash --port /dev/ttyUSB1 --baud 921600
tools/board.sh wroverkit run --release -- --port /dev/ttyUSB1
tools/board.sh audiokit run --release -- --port /dev/ttyUSB0      # songs: microSD card
tools/songs.sh yds3 build && tools/songs.sh yds3 flash --port /dev/ttyACM2 --baud 921600
tools/board.sh yds3 run --release -- --port /dev/ttyACM2          # the CH343 "COM" port
```

The ESP-WROVER-KIT's FT2232 gives two ports: `/dev/ttyUSB0` is JTAG,
`/dev/ttyUSB1` the console and flashing port. Its microSD card-detect
switch shares GPIO21 with the LCD's D/C line: with a card in the slot the
LCD stays white (the firmware logs a warning) — take the card out.

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

README screenshots (`doc/images/`): `tools/make_screenshots.sh` renders the
Tab5 pages from the real library (`tests/ui_preview`, bin `shots`: library,
search, globe, Learn, three sheet styles, settings) and a collage of the
minimal UI (`tests/mini_preview`).

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
  ├─ dual.rs      (headless + USB host) both at once, USB preferred
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
* **Headless UI** (`headless.rs`, `web.rs`, `web/index.html`): a `Coach`
  (playlist = the current song's folder, mode, hands, tempo, speaker) shared
  by the key loop and the HTTP handlers; tones go straight to the synth.
  The page is one HTML file gzipped by build.rs (11.6 → 4 KB) and polls
  `/api/state` once a second; folder listings and searches carry a
  sequence number so a slow, earlier answer cannot overwrite a newer one.
  Wi-Fi access point at 192.168.4.1 (esp-idf-svc's default subnet is
  192.168.71.x) with a captive-portal DNS. The ESP32's internal RAM is the
  limit with Wi-Fi + BLE + an HTTP task: every plain malloc goes to PSRAM
  first (`SPIRAM_MALLOC_ALWAYSINTERNAL=0` — the search index alone is
  thousands of small strings), threads that never write flash keep their
  stacks in PSRAM (`threads.rs`; on the ESP32 PSRAM is off during flash
  writes), small Wi-Fi buffer pools, and the HTTP server starts before
  Wi-Fi (its start-up fragments internal RAM): 72 KB internal free with
  everything running. Radio sharing: preferring BT starved Wi-Fi (8 s
  page loads); instead the piano's own renegotiation to a 10 ms interval
  with a 500 ms supervision timeout is answered with a request for
  30–50 ms and 4 s — page requests then take 0.1–0.4 s and the link
  rides out busy moments. The ES8388 and the 1-bit SD card come up in a
  small C component (`components/audiokit_bsp`). The speaker amplifiers
  hang on the ES8388's LOUT2/ROUT2, which esp_codec_dev leaves at −30 dB
  (it sets only the headphone outputs to 0 dB) and offers no register
  write for (`esp_codec_dev_write_reg` silently does nothing on this
  codec): the BSP sets them to +3 dB over the codec's I²C control
  interface. Output is stereo (one channel per amplifier) and the synth
  runs at twice its default gain (`SYNTH_GAIN` in the board profile).
* **BLE on the original ESP32** (CYD, WROVER-KIT, Audio Kit): their
  sdkconfig turns NimBLE's host flow control off. ESP-IDF enables it for
  this chip only, and with it the controller stops passing the piano's
  packets to the host after as many as the host has ACL buffers (24):
  the credits never come back. Key presses then stop arriving a second or
  so into playing, the link times out (0x208) and every later connection
  gets no ATT answers until a reset. `ble_midi.rs` logs what such a drop
  looks like: a 30 s traffic line (writes, notifications, busy retries,
  RSSI), and on a drop or failed connect how long ago each direction last
  moved plus NimBLE's buffer pools; a NimBLE self-reset is logged too.
* **Piano-key remote** (`piano_core::controls`, `player.rs`, `browse.rs`):
  the player checks every key press against the control map — any time
  nothing plays, mid-song only if the song has no note of that key, in the
  learner's hands, within −2…+3 s of its position (`controls::note_near`,
  a binary search into the sorted notes) — lights the key for 600 ms, keeps the
  press (and its release) from the engine and hands the control to the
  UI over a channel. Each UI class applies it its own way (full UI: opens
  the song's folder in the library and the Play page; mini: the play
  screen; headless: the `Coach`); song and folder moves are shared in
  `browse.rs`, which sees the card or the song pack as one folder tree
  (next / previous song, next / previous top-level category with a song
  drawn by the song counts below it — exact from the pack's per-folder
  totals, from the "（41）" in folder names on a card — random song, the
  儿时回忆 boot song, and `FolderShuffle`, which
  watches the player's state and loaded song to start the next shuffled
  one and notices when another song was loaded). Every board boots with
  `player::boot_options()` (the saved options in Follow) and starts the
  boot song when the piano connects.
* **USB and BLE together** (`dual.rs`, YD-ESP32-S3): both transports
  start; a router thread sends the player's messages to USB while the
  piano is on the cable, else to BLE, and drops BLE key events while USB
  is up (the piano reports each key on both links). The combined status
  names the link ("CHMidi-2.3 (USB)") and tells the LED which colour to
  show. The S3's full-speed OTG host comes up in `components/yds3_bsp`
  (`usb_host_install` plus its library task), as does the WS2812
  (`led_strip` over RMT). The console is on UART0 through the CH343,
  since the native USB port's PHY belongs to the OTG host.
* **Headless library from flash** (YD-ESP32-S3): with no card the song
  pack is the library, shown to the web page as the same folder tree
  under a virtual `/songs` root; `headless.rs` lists pack folders, maps a
  song's path to its `/pack/<index>` entry for the player and builds the
  search index from the pack (2449 songs in 1.4 s). Internal RAM: every
  plain malloc goes to PSRAM (`SPIRAM_MALLOC_ALWAYSINTERNAL=0`; at 4096
  the index's small strings ate all of it and Wi-Fi start-up panicked),
  leaving 89 KB internal free with Wi-Fi, BLE and USB host running.
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
| Boot, display orientation and colours | ✔ | ✔ (after un-mirroring X; a second unit needs Mirror display on) |
| Song pack mounted | ✔ 2449 songs, 8.9 MB | ✔ 776 songs, 1.33 MB |
| BLE: connects to GZUT-MIDI by itself | ✔ | ✔ (~2.5 s after boot) |
| Learn: LEDs light, playing advances | ✔ | ✔ |
| BOOT tap / double tap / hold | ✔ | ✔ |
| Touch: tap rows, swipe scroll, swipe-right back | — | ✔ |
| Piano keys A0/B0/C1/D1 as menu keys | ✔ | not separately tried (same code) |
| Heap | 8.3 MB free (PSRAM) | 155 KB idle; 88 KB with a 761-note song, 40 KB low-water |
| App image | 2.9 MB of 4 MB | 2.3 MB of 2.5 MB (Slint built for size) |

ESP32-Audio-Kit (ESP32-A1S ES8388, ESP32 rev 3.1, 4 MB flash, 8 MB PSRAM
of which 4 MB mapped): microSD (2449 songs indexed in 11 s), ES8388
speaker (loud and clean once LOUT2/ROUT2 were raised from −30 dB), BLE,
the six keys with beeps, the Wi-Fi page from a phone
(captive portal) and from a PC (page 0.2–0.3 s, state 0.1–0.4 s) ✔; app
1.6 MB of 3.9 MB.

ESP-WROVER-KIT v4.1 (ESP32 rev 1.0, 4 MB flash, ILI9341 detected by its
zero ID): display, 776 songs, BLE, BOOT navigation and Learn ✔; app
2.27 MB of 2.5 MB, 145 KB heap free (PSRAM stays off: ESP32 rev 1 needs
a cache workaround Rust code is not built with). Found on the way: a
4 KB main-task stack overflowed on this Xtensa chip (now 8 KB; it is
freed after start-up), and a microSD card blanks the LCD (see above).

ESP32-2432S028R (ESP32-WROOM-32, ESP32 rev 3, 4 MB flash, no PSRAM): display,
touch, 776 songs, the boot song in Follow ✔. BLE under two-way traffic
(2026-10-16): with host flow control on, the piano's key presses stopped
after ~17 notifications, ~1 s into playing, and the link dropped 10 s
later; with it off, 14 minutes of Follow with both hands and song
changes from the piano (1869 notifications in, 6167 writes out) without
a drop, no busy retries, RSSI −60 dBm ✔. Audio Kit, same test with
flow control off: 3½ minutes, four songs started from the piano, 542
notifications in, 1888 writes out, no drop or busy retry at a weak
−86…−94 dBm ✔ (before, it dropped the same way as the CYD).
WROVER-KIT (12 ACL buffers, so with flow control it would stall after
~12 packets), same test: 10 minutes, nine songs started from the piano,
1340 notifications in, 6328 writes out, no drop or busy retry, −74…−83
dBm ✔.

LILYGO T-Display-S3 (ESP32-S3R8 rev 0.2, no touch): display (8-bit
parallel ST7789, vendor init sequence) right way up and in colour, 2449
songs mounted, BLE connects, KEY / BOOT navigation and Learn ✔; app
2.9 MB of 4 MB, 8.3 MB heap free. The touch variant's CST816 is probed
at boot; its landscape mapping is untested.

VCC-GND YD-ESP32-S3 V1.4 (ESP32-S3 rev 0.2, N16R8): the piano on the
USB-C OTG port (`CHMidi-2.3`, 5022:0716) and over BLE at once, Learn on
小星星 over USB, unplugging the cable mid-song switches to Bluetooth and
the song carries on, plugging it back switches to USB, BOOT tap / double
tap / hold, RGB LED colours, the web page, folder list and pinyin search
from a PC (song pack: 2449 songs, 9.1 MB) ✔; app 2.5 MB of 4 MB.

Piano-key remote (Tab5 over Bluetooth): boots into Follow with a random
儿时回忆 song that starts when the piano connects; every control key
(songs, folders, random, play/pause, restart, tempo, modes, hands,
bookmark) does what the table says ✔; shuffle-play (G#1): starts with the
playing song, B0 skips, the next song starts by itself after one ends ✔.
ESP32-1732S019 (mini UI, 2026-10-16): A0 / B0, G1, F1 / E1 by category
(儿时回忆 → 影视剧场 → 流行空间 → 红色歌谣 → 经典乐章 → 儿时回忆 and back),
C1, D1, A#0 / C#1, G7 / A7 / B7, D7 / E7 / F7, G#1 shuffle on / B0 skip /
off ✔, then two-way playing: 18½ minutes connected, 679 notifications in,
7769 writes out, no drop at −73…−95 dBm ✔. The song keys also drove the
BLE tests on the CYD, Audio Kit and WROVER-KIT (still with the old
folder stepping). Not yet tried: shuffle's automatic next song on the
small and headless boards. Tab5 (microSD): F1 / E1 by category ✔ —
儿时回忆 → 影视剧场 → 流行空间 → 红色歌谣 → 经典乐章 → 儿时回忆 → … (the
first try picked from the whole library: the card's `midi/` holds only
the JustPiano folder, now stepped into). Mid-song window (Tab5, 山丹丹开花
红艳艳 in Listen): F1 pressed during its F1 passage at 1:56–2:03 was a
note each time (logged −0.3…−1.5 s), F1 at ~2:15 jumped to 经典乐章 ✔.

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
