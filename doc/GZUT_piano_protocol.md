# GZUT 88-key LED piano — USB / BLE protocol

Reverse-engineered description of the MIDI interfaces of the 88-key
"GZUT" / "GZU-tek" digital piano with a red LED above every key, so that
new host applications (PC, Android, browser) or firmware (ESP32) can play
notes, read the keyboard and drive the LEDs for a "learning mode".

Status legend used throughout:

| Tag            | Meaning                                                           |
|----------------|-------------------------------------------------------------------|
| **[verified]** | Tested live on 2026-10-15 (see `test_log_2026-10-15.md`)          |
| **[notes]**    | From earlier captures in `~/a/Documents_org` (not re-tested)      |
| **[hypothesis]** | Consistent with observations but not proven                     |

---

## 1. Physical device and interfaces

Retail spec (eBay listing, see `Piano_projects_Android.org`): 88 keys A0–C8,
LCD, 128 tones, 4 metronome patterns (1/4, 2/4, 3/4, 4/4), 30 demo songs,
2000 mAh battery, USB-C MIDI, Bluetooth. Works with "POP Piano"
(abcpiano), Bubble Piano, Everyone Piano.

The piano exposes **three** host interfaces:

| Interface    | Name                      | Transport                     | Purpose                         |
|--------------|---------------------------|-------------------------------|---------------------------------|
| USB-C        | `GZU-tek CHMidi-2.3`      | USB-MIDI 1.0 class-compliant  | MIDI in/out, LED control        |
| BLE          | `GZUT-MIDI`               | BLE-MIDI GATT service         | MIDI in/out, LED control        |
| BT Classic   | `GZUT-MUSIC`              | BR/EDR audio (A2DP sink)      | Stream audio to piano speakers  |

`GZUT-MIDI` and `GZUT-MUSIC` are the same dual-mode radio: their public
addresses differ only in the last bit (`7E:5F:FA:08:FC:BF` vs `…:BE`).

Both MIDI interfaces carry **the same MIDI message set** (section 3);
USB and BLE can be connected at the same time and each delivers the
local keyboard events **[verified]**.

---

## 2. Transports

### 2.1 USB-MIDI

**[verified]** (`lsusb -v -d 5022:0716`)

| Field              | Value                                         |
|--------------------|-----------------------------------------------|
| VID:PID            | `0x5022:0x0716`                               |
| Manufacturer       | `GZU-tek`                                     |
| Product            | `CHMidi-2.3`                                  |
| Serial             | `20201104`                                    |
| USB                | 2.0, Full Speed, bus powered, 100 mA          |
| Interfaces         | #0 Audio Control, #1 MIDI Streaming           |
| Jacks              | 1 embedded IN (id 1), 1 embedded OUT (id 3)   |
| Endpoints          | EP 0x02 OUT bulk 64 B, EP 0x82 IN bulk 64 B    |
| Cable number       | 0 (single port)                               |

Standard class driver works everywhere (Linux `snd-usb-audio` prints
`Quirk or no altset; falling back to MIDI 1.0`, harmless). On Linux the
port appears as ALSA rawmidi `hw:<card>,0,0` and sequencer client
`CHMidi-2.3`. (The printed manual calls the Windows port "Midi11-2.0".)

Behaviour notes:

* No Active Sensing (`0xFE`) or clock is sent while idle **[verified]**.
* Messages sent by the host are **not echoed** back **[verified]**.
* **Burst limit [verified]:** 88 Note On messages written back-to-back in
  one buffer (352 bytes of USB-MIDI event packets) → only the *upper*
  keys lit; the earliest events were lost. The same 88 notes sent as
  groups of 8 with 20 ms gaps all lit. Pace USB output to roughly
  **≤ 8 messages per 20 ms** when sending large LED updates (the exact
  limit was not searched for). The Tab5 firmware sends 8 events per
  bulk transfer, 12 ms apart, and lights all 88 keys **[verified]**.

USB-MIDI event packet format (for ESP32-S2/S3 USB host or raw libusb):
`[CIN | cable<<4, status, data1, data2]`, e.g. Note On C4 vel 1 on
cable 0 = `09 90 3C 01`, Note Off = `08 80 3C 00`.

### 2.2 BLE-MIDI (`GZUT-MIDI`)

**Advertising [verified/notes]**

* Public address (this unit) `7E:5F:FA:08:FC:BF`, local name `GZUT-MIDI`.
* Flags `0x06`, 128-bit service UUID `03B80E5A-EDE8-4B33-A751-6CE34EC4C700`,
  manufacturer data company `0x0000` payload `00 00 00 00`.
* nRF Connect raw (adv + scan response):
  `02 01 06 | 11 07 00C7C44EE36C51A7334BE8ED5A0EB803 | 07 FF 0000 00000000 | 0A 09 "GZUT-MIDI"`
* Scanning by name `GZUT-MIDI` or by the MIDI service UUID both work.

**GATT table [verified]**

| Handle  | Item                                                       | Props                    | Value seen          |
|---------|------------------------------------------------------------|--------------------------|---------------------|
| 0x0001  | Service 0x1800 Generic Access                              |                          |                     |
| 0x0003  | Device Name 0x2A00                                         | read                     | *empty*             |
| 0x0005  | Appearance 0x2A01                                          | read                     | `00 00`             |
| 0x0006  | Service 0x1801 Generic Attribute                           |                          |                     |
| 0x0007  | Service `03B80E5A-EDE8-4B33-A751-6CE34EC4C700` (BLE-MIDI)  |                          |                     |
| 0x0009  | Char `7772E5DB-3868-4112-A1A9-F2669D106BF3` (MIDI I/O)     | read, write-no-rsp, notify | read → `"01234567"` |
| 0x000A  | CCCD 0x2902                                                | read/write               |                     |

* ATT MTU negotiated: 503 (older notes) / 508 (BlueZ 5.85 today).
* Writes must be **Write Without Response** (no write-with-response flag).
* Just-Works pairing/bonding is accepted. Whether pairing is *required*
  is not established (writes worked while paired; the apps connect without
  a pairing UI) **[hypothesis: not required]**.
* Device Name characteristic is empty → take the name from advertising.

**Packet format — the important quirk [verified]**

The piano does **not** implement BLE-MIDI timestamps. Its parser does:

1. Drop the whole packet unless byte 0 (header) is **exactly `0x80`**.
2. Treat every following byte as a **plain MIDI 1.0 byte stream**
   (status bytes, running status, data bytes). A BLE-MIDI timestamp byte
   (`0x80`–`0xFF`) is therefore interpreted as a *MIDI status byte*.

Consequences:

| Packet sent                         | Result                          |
|-------------------------------------|---------------------------------|
| `80 80 90 3C 01`                    | C4 LED on ✔                     |
| `80 81 90 43 01`                    | ✔ (`81` = Note Off status, superseded by `90`) |
| `80 B0 90 41 01`                    | ✔ (`B0` superseded by `90`)     |
| `82 80 90 45 01`                    | ✘ dropped (header ≠ 0x80)       |
| `82 B0 90 40 01`                    | ✘ dropped                       |
| `80 80 90 3C 01 80 90 40 01 81 43 01` | C4 ✔, E4 ✔, G4 ✘ (`81 43 01` parsed as **Note Off** G4) |
| `80 80 90 15 01 16 01 … 6C 01` (179 B, running status, 88 notes) | all 88 LEDs on ✔ |

Rules for a host/ESP32 that wants reliable delivery:

* Always start a packet with `80 80` (header 0, timestamp 0).
* After that, send ordinary MIDI: full status bytes or running status.
  **Do not insert per-message timestamp bytes**; if you must, use a
  timestamp byte that is immediately followed by a full status byte
  (e.g. `80 90 …`) — never timestamp + running-status data.
* SysEx is fine (`80 80 F0 … F7`).
* Keep a single write ≤ 255 bytes. Earlier notes: 85 full Note On
  messages (2 + 85×3 = 257 bytes) worked, 86 (260 bytes) failed even with
  MTU 503. With running status all 88 LEDs fit in 179 bytes.
* Unlike USB, one large BLE write of 88 notes did **not** lose events.

Stacks that encode real timestamps will mostly fail:

* **Linux BlueZ MIDI plugin** (ALSA client "GZUT-MIDI Bluetooth") sends
  headers like `B8 F2 …`, `82 B0 …` → only packets sent in the ~128 ms
  window every 8.192 s where the header is `0x80` get through (≈1.6 %).
  Observed: no LED lit from 9 writes. Workaround: talk GATT directly
  (run `bluetoothd -P midi`, or use a user-space BLE library) and build
  packets yourself.
* **ESP32-P4 + NimBLE (Tab5 firmware, 2026-10-15)**: raw
  write-without-response packets `80 80 <midi…>` up to 257 bytes (MTU
  260) drive all 88 LEDs and Learn mode reliably **[verified]**.
* **Android `MidiManager.openBluetoothDevice`** uses the same timestamped
  encoding **[hypothesis: same failure; affects BleUsbMidiPlayer]**.
* Web Bluetooth apps that write `80 80 …` themselves (pianoplay fork,
  midi-piano) work.

**Notifications from the piano [verified/notes]**

One MIDI message per notification, always with zero timestamps:

```
80 80 90 15 37          Note On  A0 vel 55
80 80 80 15 00          Note Off A0 (real Note Off status, vel 0)
80 80 F0 52 4B 15 00 00 00 80 F7   SysEx (timestamp 0x80 before F7, per spec)
```

Enable notifications by writing `01 00` to the CCCD (0x000A).

### 2.3 `GZUT-MUSIC` (Bluetooth Classic audio)

**[verified]** Class of Device `0x340404` = Audio/Video, Wearable Headset;
service classes Audio, Object Transfer, Rendering. Manufacturer data
key `0x5754`, value `53 4D 4F AA 58` ("SMO.X"). It is a normal A2DP
speaker: pair from the phone/PC Bluetooth settings and play audio; it is
**not** BLE and carries no MIDI. SDP browsing without pairing returned
nothing. An ESP32 (original ESP32 only — S3/C3 have no BR/EDR) could act
as an A2DP source to it, but that is independent of MIDI.

---

## 3. MIDI message set

All note numbers are standard MIDI: A0 = 21 (`0x15`), middle C (C4) = 60
(`0x3C`), C8 = 108 (`0x6C`).

### 3.1 Piano → host (keyboard)

| Message              | Bytes                | Notes                                   |
|----------------------|----------------------|-----------------------------------------|
| Key down             | `90 nn vv`           | channel 1 only                          |
| Key up               | `80 nn 00`           | real Note Off, velocity 0               |
| Panel parameter      | `F0 52 4B pp vv 00 00 F7` | see 3.3                            |

Velocity is quantised to a few levels: observed `0x11` (17), `0x1E` (30),
`0x23` (35), `0x28` (40), `0x37` (55), `0x41` (65) **[verified]**.
Do not rely on the full 1–127 range for scoring/dynamics.

Sustain pedal / other CCs were not captured.

### 3.2 Host → piano (sound and LEDs)

The piano lights a key's LED whenever it receives a Note On for that key
and turns it off on Note Off. Very low velocities light the LED without
an audible note, which is how learning apps show "next key" hints.

| Purpose                         | Bytes             | Result                                       |
|---------------------------------|-------------------|----------------------------------------------|
| **LED on, silent** (hint)       | `90 nn 01`        | LED on, no sound **[verified]**              |
| Play note (LED follows)         | `90 nn vv` (vv≥16)| sound + LED **[verified]** (vel 100)          |
| LED/note off                    | `80 nn 00`        | LED off **[verified]**                       |
| LED/note off (alt.)             | `90 nn 00`        | LED off **[verified]**                       |
| All LEDs/notes off              | `B0 7B 00` (CC 123) | clears every LED, incl. all 88 **[verified]** |
| Change tone of host-played notes| `C0 pp`           | GM-like: `C0 18` sounded like nylon guitar; panel display unchanged **[verified]** |

Details **[verified]**:

* Audibility threshold: velocity 1, 2, 8 silent; 16 audible. Use **1** for
  LED-only hints (POP-Piano-style apps and the pianoplay fork use 1–2).
* Any MIDI channel lights LEDs (tested channel 1 and 2). Recommendation:
  use channel 1 for consistency with the keyboard's own output.
* A host-lit LED is **independent of the physical key**: pressing and
  releasing a lit key does not turn the host LED off; the host must send
  the Note Off. The local key press is still reported normally.
* LEDs stay on until turned off (20 s holds tested, no timeout seen).
* Running status works on both transports (`90 3C 01 40 01 43 01`).
* The standard Universal Identity Request (`F0 7E 7F 06 01 F7`) gets **no
  reply**.
* Writing the panel SysEx back (`F0 52 4B 2A 08 00 00 F7`) did **not**
  change the volume — the vendor SysEx appears to be output-only.

Because "LED on" is literally "Note On", a host that both plays notes and
shows hints must track which keys it lit; releasing a played note also
turns its hint off.

### 3.3 Vendor SysEx (panel state, piano → host)

Format: `F0 52 4B <param> <value> 00 00 F7` (8 bytes). `0x52` is
nominally Zoom Corporation's manufacturer ID; this firmware just uses it
(`52 4B` = ASCII "RK").

| param | Meaning                                  | Values seen          | Evidence |
|-------|------------------------------------------|----------------------|----------|
| `0x2A`| Master volume                            | 4…11; display shows same number | **[verified]** volume buttons 6→5→4→5 |
| `0x2B`| Metronome / beat pattern                 | 1, 2, 3, 4           | **[hypothesis]** captured while user operated metronome |
| `0x15`| "Home" display value — current tone no.  | 0…15                 | **[hypothesis]** emitted ~2 s after any other parameter change (display returns to home), on BLE connect (`15 00`), and stepped 1→15 while panel +/- was pressed |

Typical sequence for one volume press:

```
F0 52 4B 2A 05 00 00 F7     volume = 5
   (≈2 s later)
F0 52 4B 15 00 00 00 F7     display back to home, tone = 0
```

Unknown SysEx strings in the old notes (possibly sniffed from an app,
untested): `F0 4D 4C 4C 45 40 11 F7` ("MLLE@"),
`F0 4D 4C 8C 49 30 81 87 80 E0 F0 47 48 4C 45 40 21 F7` (malformed /
interleaved BLE timestamps).

---

## 4. Recipes

### 4.1 Learning mode (show next notes)

```
on step k:
    off = lit_keys - next_keys
    on  = next_keys - lit_keys
    send 80 80 | 80 n1 00 n2 00 …        (Note Off, running status)   ; BLE
    send 80 80 | 90 m1 01 m2 01 …        (Note On vel 1, running status)
    lit_keys = next_keys
on user key press (90 nn vv from piano): compare with next_keys
on stop: send B0 7B 00
```

Over USB drop the `80 80` prefix and pace large updates (≤ 8 msgs / 20 ms).

### 4.2 Linux shell

```bash
amidi -l                                         # find hw:X,0,0
amidi -p hw:3,0,0 -S '90 3C 01'                  # C4 LED on (silent)
amidi -p hw:3,0,0 -S '80 3C 00'                  # C4 LED off
amidi -p hw:3,0,0 -S 'B0 7B 00'                  # all off
amidi -p hw:3,0,0 -d                             # dump keyboard/panel output
```

BLE without the BlueZ MIDI plugin (raw GATT, header must be `80`):

```bash
gattcat write 7E:5F:FA:08:FC:BF 03b80e5a-ede8-4b33-a751-6ce34ec4c700 \
        7772e5db-3868-4112-a1a9-f2669d106bf3 8080903C01
```

When BlueZ is built with its MIDI plugin (Ubuntu's 5.85 is), the plugin claims the characteristic
(D-Bus `WriteValue` returns "Operation Not Authorized") and its ALSA port
sends timestamped headers that the piano drops. Temporarily:
`sudo systemctl stop bluetooth && sudo /usr/libexec/bluetooth/bluetoothd -n -P midi`.

### 4.3 Python, USB (no extra packages)

```python
import os, time
fd = os.open("/dev/snd/midiC3D0", os.O_WRONLY)       # card 3, device 0
os.write(fd, bytes([0x90, 60, 1])); time.sleep(1)   # C4 LED on (silent)
os.write(fd, bytes([0x80, 60, 0]))                  # C4 LED off
for n in range(21, 109, 8):                           # light all, paced
    os.write(fd, b"".join(bytes([0x90, k, 1]) for k in range(n, min(n + 8, 109))))
    time.sleep(0.02)
os.write(fd, bytes([0xB0, 0x7B, 0x00]))              # all off
```

### 4.4 ESP32 as BLE central (NimBLE-Arduino sketch outline)

Untested on ESP32 — follows directly from section 2.2.

```cpp
static const NimBLEUUID MIDI_SVC("03b80e5a-ede8-4b33-a751-6ce34ec4c700");
static const NimBLEUUID MIDI_CHR("7772e5db-3868-4112-a1a9-f2669d106bf3");

// scan for name "GZUT-MIDI" or MIDI_SVC, connect, then:
NimBLERemoteCharacteristic* chr = client->getService(MIDI_SVC)->getCharacteristic(MIDI_CHR);
chr->subscribe(true, [](NimBLERemoteCharacteristic*, uint8_t* d, size_t n, bool) {
    // d[0]==0x80, d[1]==0x80, d[2..] = one plain MIDI message
});

void sendMidi(const uint8_t* midi, size_t len) {     // len <= 253
    uint8_t pkt[255] = {0x80, 0x80};                  // header MUST be 0x80
    memcpy(pkt + 2, midi, len);
    chr->writeValue(pkt, len + 2, /*response=*/false);
}
// LED on C4 (silent):  uint8_t m[] = {0x90, 0x3C, 0x01}; sendMidi(m, 3);
```

Do **not** use generic BLE-MIDI libraries that insert real timestamps
(e.g. the default encoders of Arduino BLE-MIDI / Android / BlueZ) unless
you can force the header and timestamp bytes to `0x80`.

### 4.5 ESP32 over USB

ESP32-S2/S3 can act as USB host (TinyUSB/ESP-IDF `usb_host` with a
MIDI class driver) and talk to `5022:0716` using 4-byte USB-MIDI event
packets on EP 0x02 / 0x82 (section 2.1). Remember the burst limit.
The original ESP32 has no USB OTG.

---

## 5. Open questions / next experiments

* Exact USB burst limit and whether it also drops *sound* events.
* Is BLE pairing required for writes? (Test from an unpaired ESP32.)
* Confirm `0x15` = tone number and `0x2B` = metronome by stepping each
  control once with the display visible.
* Sustain pedal message (expect `B0 40 7F/00`), demo-song / record
  buttons, transpose, other panel SysEx parameters.
* Does the piano honour CC 7 (volume), CC 64 from the host, pitch bend?
* Meaning of the `F0 4D 4C …` strings from the old notes (POP Piano app?).
* Whether Android `MidiManager` BLE output really fails (sniff with
  Android HCI snoop log).

---

## 6. Notes on existing code in this workspace

* `~/w/piano/pianoplay` (`home.page.ts`): `TurnOnLedNote` = `90 nn 01`,
  `TurnOffLedNote` = `80 nn 00`; BLE writes prefix `80 80` — matches this
  spec. The commented-out batched `turnOnAllRequiredLeds` "did not work"
  because `Array.from(map.keys()).map(([key]) => …)` destructures the
  **first character** of each key string (e.g. `"60"` → `"6"`), producing
  wrong note numbers — not a device limitation. Batching with running
  status is verified to work.
* `~/w/piano/midi-piano` (`js/app.js`): builds `80 80 <midi>` packets and
  uses `writeValueWithoutResponse` — compatible.
* `~/w/piano/BleUsbMidiPlayer`: BLE via `MidiManager.openBluetoothDevice`
  → Android's timestamped encoder; likely to lose most BLE packets on
  this piano (section 2.2). USB path is fine.
