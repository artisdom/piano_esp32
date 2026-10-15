# Boards

Piano Coach builds for one board at a time. Each board is:

| Piece | Where | What |
|---|---|---|
| Profile | `boards/<board>/board.env` | `FEATURE`, `MCU`, ESP-IDF source (`ESP_IDF_VERSION` *or* `IDF_PATH_DEFAULT`), `FLASHER`, `UI` (`full` / `mini` / `headless`), `HAS` (`usb sd speaker`), optional `TARGET` (Rust target, default `riscv32imafc-esp-espidf`), `TOOLCHAIN` (rustup toolchain, e.g. `esp198` for Xtensa), `PICOLIBC=1`, `PATCH_EXFAT=1`, `SONGPACK_ARGS` (song pack budget), `CARGO_CONFIG` (extra `--config` values) |
| ESP-IDF config | `boards/<board>/sdkconfig.defaults` | the board's full sdkconfig baseline |
| Native components | `boards/<board>/esp-idf-components.toml` | BSP and other components (paths relative to the file) |
| BSP | `components/<bsp>/` | vendored or forked board support package (no LVGL); small boards may use ESP-IDF's own drivers or a registry component instead |
| Partitions | `boards/<board>/partitions.csv` | optional; boards without microSD need a `songs` data partition (subtype `undefined`) |
| Glyphs | `boards/<board>/glyphs.slint` | UI=mini only: written by `tools/songs.sh <board> build` |
| Rust board module | `src/board/<board>/mod.rs` | the items `src/board/contract.rs` requires |
| Cargo feature | `Cargo.toml` `[features]` | `board-<board> = []` (keep it empty) |
| Bindgen | `src/bindings.h` | a `#elif CONFIG_IDF_TARGET_…` branch with the BSP headers |
| Selection | `src/board/mod.rs` | a `#[cfg(feature = "board-<board>")] #[path = "<board>/mod.rs"] mod imp;` line |

Build / flash:

```bash
tools/board.sh <board> build --release
tools/board.sh <board> run --release -- --port /dev/ttyUSB3
```

Song pack (UI=mini boards):

```bash
tools/songs.sh <board> build            # target/<board>/songs.bin + boards/<board>/glyphs.slint
tools/songs.sh <board> flash --port /dev/ttyACM0
```

`tools/board.sh` lists the boards it finds (every `boards/*/board.env`).
`build.rs` reads the same files: it rejects zero or several board features,
an `MCU` that does not match the feature, a profile without a Rust
module, and `UI=full` without `HAS="usb sd speaker"`. It turns `UI` and
`HAS` into `cfg(ui_full)` / `cfg(ui_mini)` / `cfg(has_*)` and compiles
`ui/app.slint` or `ui/mini/app.slint` (with `@board/` mapped to the
profile directory). A bare `cargo build` builds the Tab5 (defaults in
`.cargo/config.toml`).

## Supported boards

| Board | Directory | MCU | ESP-IDF | Display | MIDI transports |
|---|---|---|---|---|---|
| M5Stack Tab5 | `tab5` | ESP32-P4 | v5.5.3 | 1280×720 MIPI-DSI, rotated 270°, scale 1 | USB-A host; BLE via ESP32-C6 (ESP-Hosted) |
| ESP32-S31-Korvo-1 V1.1 | `korvo` | ESP32-S31 | master (≥ 6.1) | 800×480 RGB (SUB3), scale 0.625 | USB-A host (HS); native BLE |
| ESP32-1732S019 | `1732s019` | ESP32-S3 (Xtensa) | v5.5.3 | 1.9″ 320×170 ST7789 SPI, minimal UI, no touch | native BLE |
| ESP32-2424S012C | `2424s012` | ESP32-C3 | v5.5.3 | 1.28″ round 240×240 GC9A01 SPI, minimal UI, CST816D touch | native BLE |
| ESP-WROVER-KIT v4.1 | `wroverkit` | ESP32 (Xtensa) | v5.5.3 | 3.2″ 320×240 ILI9341 / ST7789V SPI (ID probe), minimal UI at 1.25× | native BLE |
| Ai-Thinker ESP32-Audio-Kit | `audiokit` | ESP32 (Xtensa) | v5.5.3 | none: headless (keys, tones, Wi-Fi web page) | native BLE |
| LILYGO T-Display-S3 | `tdisplays3` | ESP32-S3 (Xtensa) | v5.5.3 | 1.9″ 320×170 ST7789 on an 8-bit i80 bus, minimal UI, two keys, optional CST816 touch | native BLE |

## Porting a new board — checklist

1. **Profile.** Copy the closest `boards/<board>/` and edit `board.env`
   (feature name, `MCU`, ESP-IDF source, flasher: `espflash`, or `idf` where
   espflash cannot package the image), `sdkconfig.defaults` (flash/PSRAM,
   `CONFIG_FREERTOS_HZ=1000`, FatFs LFN + UTF-8, NimBLE central-only with
   `CONFIG_BT_NIMBLE_LOG_LEVEL_WARNING`, USB host) and
   `esp-idf-components.toml`.
2. **BSP.** Add it under `components/`. It must not pull in LVGL (Slint
   renders into the framebuffer itself). Add its headers to `src/bindings.h`.
3. **Rust module** `src/board/<board>/mod.rs`, providing everything in
   `contract.rs`. Every board: `CAPS` (name, backlight, power latch,
   USB/BLE MIDI, buttons, touch), `early_init`, `init_power`,
   `set_backlight`, `power_off`, `reboot`, `bt_controller_start`,
   `poll_buttons` (a single key can use `board::key::Key` for
   tap / double tap / hold; boards with two keys report `Button::Nav*`).
   `CAPS.key_hint` / `wake_hint` are shown in the mini UI.

   UI=mini (small SPI panel): `LCD_WIDTH` / `LCD_HEIGHT` (as the UI sees
   them), `ROUND`, `UI_SCALE` (1.0; larger panels draw the 170-row bar
   layout bigger, e.g. 1.25 for 240 rows), `lcd_init` (bus + panel via `board::spi_lcd`, rotation,
   colour order, backlight; `spi_lcd::open_io` for SPI panels,
   `open_io_i80` for 8-bit parallel ones), `lcd_draw` (usually
   `SpiLcd::draw`), `touch_point` (`None` without touch; `board::cst816`
   reads CST816 controllers). Add a `songs` partition and a
   `SONGPACK_ARGS` budget that fits it, and check the UI in
   `tests/mini_preview` (add the size there if it is new).

   UI=headless (no display): `status_led`; `poll_buttons` reports numbered
   keys as `Button::Keypad(n)` / `KeypadHold(n)`.

   HAS=sd: `mount_sd`. HAS=speaker: `speaker_codec` (an esp_codec_dev
   handle), `speaker_enable`, `AUDIO_OUT_RATE` / `AUDIO_CHANNELS`
   and `SYNTH_GAIN` (the synth's master gain for that speaker; 0.8 is the
   synth's default).

   UI=full (framebuffer panel), additionally:
   * geometry — `FRAME_WIDTH/HEIGHT` (as the panel scans), `WINDOW_WIDTH/HEIGHT`
     (physical landscape), `UI_SCALE` (logical canvas = 1280 wide:
     `WINDOW_WIDTH / UI_SCALE` should be ≈ 1280, height ≥ 720),
     `RENDERING_ROTATION`, `fb_index`, `fb_rows`, `touch_to_logical`;
   * `display_init` (panel, touch, two framebuffers → `board::adopt`),
     `present`;
   * `usb_host_start`.
4. **Feature + selection** lines in `Cargo.toml` and `src/board/mod.rs`
   (and a `src/bindings.h` branch for the target).
5. **Check layout** on the PC: add the board's `(name, width, height, scale)`
   to `tests/ui_preview/src/main.rs` (full) or its size to
   `tests/mini_preview` (mini) and look at the PNGs.
6. **Bring-up order** on hardware: boot log → display (orientation,
   colours) + touch → microSD or song pack → USB MIDI (LED test in
   Settings) → speaker → BLE → globe (it paints straight into the
   framebuffer through `fb_index`, so a wrong mapping shows up there
   first). Small boards: display → keys/touch → BLE → Learn; watch the
   heap log (`ui: heap free …`) on boards without PSRAM.

Nothing outside `src/board/` may name a board; application code asks
`board::CAPS` and the facade instead.
