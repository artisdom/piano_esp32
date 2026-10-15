# Boards

Piano Coach builds for one board at a time. Each board is:

| Piece | Where | What |
|---|---|---|
| Profile | `boards/<board>/board.env` | `FEATURE`, `MCU`, ESP-IDF source (`ESP_IDF_VERSION` *or* `IDF_PATH_DEFAULT`), `FLASHER`, optional `PICOLIBC=1`, `PATCH_EXFAT=1` |
| ESP-IDF config | `boards/<board>/sdkconfig.defaults` | the board's full sdkconfig baseline |
| Native components | `boards/<board>/esp-idf-components.toml` | BSP and other components (paths relative to the file) |
| BSP | `components/<bsp>/` | vendored or forked board support package (no LVGL) |
| Rust board module | `src/board/<board>/mod.rs` | the items `src/board/contract.rs` requires |
| Cargo feature | `Cargo.toml` `[features]` | `board-<board> = []` (keep it empty) |
| Bindgen | `src/bindings.h` | a `#elif CONFIG_IDF_TARGET_…` branch with the BSP headers |
| Selection | `src/board/mod.rs` | a `#[cfg(feature = "board-<board>")] #[path = "<board>/mod.rs"] mod imp;` line |

Build / flash:

```bash
tools/board.sh <board> build --release
tools/board.sh <board> run --release -- --port /dev/ttyUSB3
```

`tools/board.sh` lists the boards it finds (every `boards/*/board.env`).
`build.rs` reads the same files: it rejects zero or several board features,
an `MCU` that does not match the feature, and a profile without a Rust
module. A bare `cargo build` builds the Tab5 (defaults in
`.cargo/config.toml`).

## Supported boards

| Board | Directory | MCU | ESP-IDF | Display | MIDI transports |
|---|---|---|---|---|---|
| M5Stack Tab5 | `tab5` | ESP32-P4 | v5.5.3 | 1280×720 MIPI-DSI, rotated 270°, scale 1 | USB-A host; BLE via ESP32-C6 (ESP-Hosted) |
| ESP32-S31-Korvo-1 V1.1 | `korvo` | ESP32-S31 | master (≥ 6.1) | 800×480 RGB (SUB3), scale 0.625 | USB-A host (HS); native BLE |

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
   `contract.rs`:
   * `CAPS` — name, backlight, power latch, USB/BLE MIDI, buttons;
   * geometry — `FRAME_WIDTH/HEIGHT` (as the panel scans), `WINDOW_WIDTH/HEIGHT`
     (physical landscape), `UI_SCALE` (logical canvas = 1280 wide:
     `WINDOW_WIDTH / UI_SCALE` should be ≈ 1280, height ≥ 720),
     `RENDERING_ROTATION`, `fb_index`, `fb_rows`, `touch_to_logical`;
   * `display_init` (panel, touch, two framebuffers → `board::adopt`),
     `present`, `set_backlight`;
   * `early_init` (bus recovery, memory policy), `init_power(transport)`,
     `mount_sd` (at `/sdcard`), `usb_host_start`, `bt_controller_start`
     (coprocessor bring-up, or nothing for an on-chip controller),
     `speaker_enable`, `AUDIO_OUT_RATE` (multiple of 24 kHz) /
     `AUDIO_CHANNELS`, `poll_buttons`, `power_off`, `reboot`.
4. **Feature + selection** lines in `Cargo.toml` and `src/board/mod.rs`.
5. **Check layout** on the PC: add the board's `(name, width, height, scale)`
   to `tests/ui_preview/src/main.rs` and look at the PNGs.
6. **Bring-up order** on hardware: boot log → display + touch → microSD →
   USB MIDI (LED test in Settings) → speaker → BLE → globe (it paints
   straight into the framebuffer through `fb_index`, so a wrong mapping
   shows up there first).

Nothing outside `src/board/` may name a board; application code asks
`board::CAPS` and the facade instead.
