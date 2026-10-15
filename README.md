# piano_esp32

Host/firmware work for the GZUT 88-key LED digital piano
(USB `GZU-tek CHMidi-2.3`, BLE `GZUT-MIDI`, BT audio `GZUT-MUSIC`).

* [doc/GZUT_piano_protocol.md](doc/GZUT_piano_protocol.md) — USB and BLE
  MIDI protocol, LED control, vendor SysEx, BLE packet quirks, recipes
  for Linux/Python/ESP32.
* [doc/test_log_2026-10-15.md](doc/test_log_2026-10-15.md) — raw captures
  and live test results the protocol document is based on.
* [doc/piano_coach_firmware.md](doc/piano_coach_firmware.md) — **Piano Coach**, the Rust
  play & learn firmware (`firmware/piano-coach`, `firmware/piano-core`):
  Learn / Follow / Listen modes on the M5Stack Tab5 and ESP32-S31-Korvo-1
  (USB or Bluetooth, songs from microSD), and a minimal UI for the small
  ESP32-1732S019 (1.9″) and round ESP32-2424S012 (1.28″) boards
  (Bluetooth, songs in flash).

Quick start (USB, Linux):

```bash
amidi -p hw:3,0,0 -S '90 3C 01'
```

lights middle C silently; `80 3C 00` turns it off; `B0 7B 00` clears all.
