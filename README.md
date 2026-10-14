# piano_esp32

Host/firmware work for the GZUT 88-key LED digital piano
(USB `GZU-tek CHMidi-2.3`, BLE `GZUT-MIDI`, BT audio `GZUT-MUSIC`).

* [doc/GZUT_piano_protocol.md](doc/GZUT_piano_protocol.md) — USB and BLE
  MIDI protocol, LED control, vendor SysEx, BLE packet quirks, recipes
  for Linux/Python/ESP32.
* [doc/test_log_2026-10-15.md](doc/test_log_2026-10-15.md) — raw captures
  and live test results the protocol document is based on.

Quick start (USB, Linux):

```bash
amidi -p hw:3,0,0 -S '90 3C 01'
```

lights middle C silently; `80 3C 00` turns it off; `B0 7B 00` clears all.
