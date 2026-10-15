# Temporary Rust ESP32-S31 compatibility

Source-pinned copies of the esp-rs crates, carrying local changes that no
release contains yet:

| crate | origin | revision |
|---|---|---|
| esp-idf-sys | git `esp-rs/esp-idf-sys` | `8369f610a31e9c90a5479ed7f366f983589dfa0a` (master, 2026-08-01) |
| esp-idf-hal | git `esp-rs/esp-idf-hal` | `f1bac2d93fe9e92a1df2ad0acd34ef702a791ba4` (master, 2026-08-01) |
| esp-idf-svc | crates.io | `0.52.1` |

## What we changed

`patches/*.diff` is the authoritative record: the exact delta between each
pinned upstream source and the tree here. Regenerate it with
`tools/vendor-diff.sh` (which re-fetches the revisions above) after touching
anything under `vendor/`, so the patches never drift from the sources. They
apply from this directory with `git apply -p1`.

Sixteen files across the three crates are modified. In summary:

- **esp-idf-sys** — teach the native builder that ESP32-S31 exists: an
  `esp32s31` chip variant on the `riscv32imafc-esp-espidf` target
  (`cargo_driver/chip.rs`), and resolve `extra_components` entries relative to
  the board profile that declared them rather than the build script's working
  directory (`cargo_driver/config.rs`), which is what makes per-board
  `boards/*/esp-idf-components.toml` work. Track changes to the profile and
  local component directories, and locate the profile directory for copying
  bootloader/table binaries with both Cargo build-directory layouts
  (`cargo_driver.rs`).
- **esp-idf-hal** — S31 GPIO/ADC/peripheral capability maps and ESP-IDF 6.2
  API adaptations (`gpio.rs`, `adc.rs`, `peripherals.rs`, `cpu.rs`, `task.rs`,
  `i2s.rs`, `lib.rs`). S31 uses the multicore ISR spinlock path in
  `interrupt.rs`; the single-core entry point aborts on hardware.
- **esp-idf-svc** — ESP-IDF 6.2 picolibc/VFS, netif, WiFi and logging
  interface changes (`io.rs`, `log.rs`, `netif.rs`, `wifi.rs`), and suppress
  the `timegm` fallback in `private/stubs.rs` when picolibc provides it.
  Initialize the new WiFi task-stack-size field from the SDK default; a zero
  value prevents the native WiFi task from starting.

Nothing else differs from upstream. Trailing-whitespace-only edits were
reverted so the patches stay reviewable.

## Removing this

Drop `[patch.crates-io]` from `Cargo.toml` and delete this directory once
upstream releases recognize `esp32s31` and ESP-IDF 6.2. Check the patches
first: anything still listed there has to land upstream or be re-applied.
