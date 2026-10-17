# VCC-GND YD-ESP32-S3 N16R8 (`yds3`)

Hardware reference for Piano Coach on the VCC-GND Studio YD-ESP32-S3
core board. It is meant to be enough to develop for this board without
the vendor material; the vendor files used are copied to
[`vendor/yds3/`](vendor/yds3/).

Labels used below: **(code)**: the firmware drives it this way and it works
on the bench unit. **(schematic)** / **(vendor doc)**: only from the vendor
schematic or images, not exercised by the firmware. **(datasheet)**: from
Espressif's ESP32-S3 / ESP32-S3-WROOM-1 datasheets.

## 1. Summary

A small (63.4 × 27.9 mm) ESP32-S3 core board with no display: an
ESP32-S3-WROOM-1 N16R8 module, two USB-C connectors (one through a CH343P
USB-UART, one on the S3's own USB pins), a WS2812 RGB LED, BOOT and RST
keys, and two 22-pin headers. Piano Coach runs **headless** on it: the
BOOT key, the RGB LED and a Wi-Fi phone page are the whole user interface;
all 2449 songs live in a song pack in flash; and the piano is reached over
**USB (the S3's native port as a USB host) and Bluetooth LE at the same
time**, USB preferred (`src/dual.rs`). The native port can only power the
piano when the board's "USB-OTG" solder jumper is bridged; the console and
flashing go through the CH343 port.

| Item | Value |
|---|---|
| Vendor / product | VCC-GND Studio (源地工作室) YD-ESP32-S3, schematic title "YD-ESP32-S3-COREBOARD V1.4" (2022-09-23) |
| Bench unit | V1.4, ESP32-S3 chip revision v0.2, N16R8 |
| Board dir / profile | `firmware/piano-coach/boards/yds3/` (`board.env`, `sdkconfig.defaults`, `partitions.csv`, `esp-idf-components.toml`) |
| Board module | `firmware/piano-coach/src/board/yds3/mod.rs`; C BSP `firmware/piano-coach/components/yds3_bsp/` |
| Cargo feature | `board-yds3` (`MCU=esp32s3`) |
| UI class / HAS | `UI=headless`, `HAS="usb"` (no `sd`, no `speaker`) |
| Module | ESP32-S3-WROOM-1-N16R8 (PCB antenna) |
| CPU / arch | ESP32-S3, dual-core Xtensa LX7, 240 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240`) |
| Rust target / toolchain | `xtensa-esp32s3-espidf`, esp-rs toolchain `esp198` (`espup install --name esp198 --toolchain-version 1.98.1.0`) |
| ESP-IDF | v5.5.3 (`ESP_IDF_VERSION` in board.env), tools shared from `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif` |
| Flash | 16 MB Quad SPI in the module; firmware uses QIO (`CONFIG_ESPTOOLPY_FLASHMODE_QIO`) |
| PSRAM | 8 MB **Octal** SPI (in-package ESP32-S3R8), 80 MHz (`CONFIG_SPIRAM_MODE_OCT`, `CONFIG_SPIRAM_SPEED_80M`) |
| USB "COM" port (console, flashing) | CH343P USB-UART, Linux sees **1a86:55d3** as `/dev/ttyACM*` (cdc_acm) |
| USB "USB" port (piano) | S3 native USB on GPIO19 (D−) / GPIO20 (D+), run as **USB OTG full-speed host**; it does not appear on the PC while Piano Coach runs |
| Bench-unit MAC | 90:70:69:31:db:3c (from the porting session; not recorded elsewhere in the repo) |
| Operating temperature | −40…65 °C for R8 modules (datasheet) |

## 2. Feature overview

| Part (ref) | Part number | Function | Pins / nets | Used by Piano Coach |
|---|---|---|---|---|
| U4 | ESP32-S3-WROOM-1-N16R8 | MCU, Wi-Fi, BLE 5, 16 MB flash, 8 MB octal PSRAM | all GPIOs; EN = `CHIP_PU` | yes |
| U2 | CH343P (WCH) | USB-UART bridge on the "COM" USB-C; powered from that port's VBUS (`VBUSB`), I/O at 3.3 V | TXD → U0RXD (GPIO44), RXD ← U0TXD (GPIO43), DTR, RTS, UD+/UD− to USB1 | yes: console + flashing (UART0) |
| Q1, Q2 + R5, R7 | SS8050-G NPN ×2, 10 kΩ | auto-reset / auto-download from DTR / RTS | → `CHIP_PU` (EN), GPIO0 | yes (espflash uses it; RTS reset for tests) |
| USB1 | TYPEC-304-BCP16 | "COM" USB-C: UART via CH343; CC1/CC2 5.1 kΩ to GND (sink) | VBUS → `VBUSB` → D1 → 5V | yes |
| USB2 | TYPEC-304-BCP16 | "USB" USB-C: S3 native USB; CC1/CC2 5.1 kΩ to GND (R9, R10) | DP → GPIO20, DN → GPIO19; VBUS → D2 → 5V | yes: OTG host for the piano |
| D1, D2, D3 | marked "D26" in the schematic (exact type unknown) | one-way 5 V feeds: USB1 VBUS → 5V, USB2 VBUS → 5V, header 5Vin → 5V | | (power path) |
| "USB-OTG" jumper | 0 Ω 0603 link position, across D2 | bridged: the board's 5V rail drives USB2's VBUS, i.e. the board **powers a device** on the native port | USB2 VBUS ↔ 5V | **yes — must be bridged** (bridged on the bench unit) |
| "IN-OUT" jumper | 0 Ω 0603 link position, across D3 | bridged: 5V rail is output on header pin J1-21 (default open: 5Vin is input only) | J1-21 ↔ 5V | no |
| "RGB" jumper | 0 Ω 0603 link (vendor calls it a "capacitor") | connects GPIO48 to the WS2812 DI; fitted by default; remove to drive an external RGB strip | GPIO48 → `RGB_CTRL` | yes (must stay fitted) |
| "USB-JTAG" link + R11 | 0 Ω 0603 position + 10 kΩ | pull-up of GPIO3 (JTAG-source strapping pin) to 3.3 V; whether fitted on the bench unit: unknown | GPIO3 | no |
| U1 | XL-5050RGBC-WS2812B | addressable RGB LED, VDD from 5V, DO unconnected | DI ← GPIO48 (via "RGB" link) | yes: status LED |
| U3 | CJ6107A33GW | 5 V → 3.3 V LDO (VDD33); C4/C3 10 µF | IN 5V, OUT VDD33 | (power) |
| PWR LED | red LED + R6 1 kΩ | 3.3 V present | VDD33 | no |
| TX / RX LEDs | blue LEDs + R1/R4 1 kΩ to VDD33 | light when U0TXD / U0RXD are low (UART activity) | GPIO43, GPIO44 | indirectly (console traffic) |
| BOOT key | ST-1185S + C8 100 nF | GPIO0 to GND; no external pull-up (internal pull-up) | GPIO0 | yes: the only user key |
| RST key | ST-1185S + C9 100 nF | `CHIP_PU` to GND; EN has R8 10 kΩ pull-up, C7 1 µF | EN | yes (reset; wakes from deep sleep) |
| J1, J2 | 2 × 22-pin 2.54 mm headers, rows 25.40 mm apart | GPIO breakout | see §3 | no (nothing attached) |

Board size: 63.39 × 27.94 mm; outer header pins 57.15 mm apart along the
board (`vendor/yds3/yd-esp32-s3_dimensions-mm.pdf`).

## 3. Pin map

### 3.1 Pins the firmware uses

| GPIO | Function on this board | Firmware use | Source |
|---|---|---|---|
| 0 | BOOT key (active low), strapping pin (boot mode) | key input with internal pull-up, polled every 15 ms (`board/key.rs`); ext0 wake source in `power_off()` | code + schematic |
| 19 | USB D− (native USB, USB2 connector) | USB OTG host (internal PHY, `skip_phy_setup = false`) | code (BSP header comment) + schematic |
| 20 | USB D+ (native USB, USB2 connector) | USB OTG host | code + schematic |
| 43 | U0TXD → CH343 RXD | UART0 console TX (ESP-IDF default UART console) | sdkconfig + schematic |
| 44 | U0RXD ← CH343 TXD | UART0 console RX | sdkconfig + schematic |
| 48 | WS2812 DI via "RGB" link | `led_strip` over RMT, 10 MHz resolution, 1 LED, GRB order | code (`yds3_bsp.c`) + schematic |
| 35, 36, 37 | octal PSRAM data / DQS inside the module | PSRAM (reserved — **do not use**, although they are on header J2) | datasheet |
| 26–34 | module flash / PSRAM bus (not brought out of the module) | flash + PSRAM | datasheet |

### 3.2 Header pinout (schematic J1 / J2, vendor pinout image)

Viewed from the top with the USB-C connectors at the bottom: J1 is the
left row, J2 the right row; pin 1 is at the antenna end.

| J1 pin | Signal | Other functions (vendor pinout) | Notes |
|---|---|---|---|
| 1 | 3V3 | | VDD33 out (LDO) |
| 2 | 3V3 | | |
| 3 | RST (`CHIP_PU` / EN) | | RST key, auto-reset transistor Q1 |
| 4 | GPIO4 | ADC1_3, TOUCH4, RTC | free |
| 5 | GPIO5 | ADC1_4, TOUCH5, RTC | free |
| 6 | GPIO6 | ADC1_5, TOUCH6, RTC | free |
| 7 | GPIO7 | ADC1_6, TOUCH7, RTC | free |
| 8 | GPIO15 | ADC2_4, U0RTS, XTAL_32K_P, RTC | free |
| 9 | GPIO16 | ADC2_5, U0CTS, XTAL_32K_N, RTC | free |
| 10 | GPIO17 | ADC2_6, U1TXD, RTC | free |
| 11 | GPIO18 | ADC2_7, U1RXD, CLK_OUT3, RTC | free |
| 12 | GPIO8 | ADC1_7, TOUCH8, RTC | free |
| 13 | GPIO3 | ADC1_2, TOUCH3, RTC | **strapping** (JTAG source); optional 10 kΩ pull-up via "USB-JTAG" link |
| 14 | GPIO46 | (vendor label "LOG") | **strapping** (boot mode, ROM log); internal pull-down at reset |
| 15 | GPIO9 | ADC1_8, TOUCH9, FSPIHD | free |
| 16 | GPIO10 | ADC1_9, TOUCH10, FSPIIO4 / FSPICS0 | free |
| 17 | GPIO11 | ADC2_0, TOUCH11, FSPIIO5 / FSPID | free |
| 18 | GPIO12 | ADC2_1, TOUCH12, FSPIIO6 / FSPICLK | free |
| 19 | GPIO13 | ADC2_2, TOUCH13, FSPIIO7 / FSPIQ | free |
| 20 | GPIO14 | ADC2_3, TOUCH14, FSPIDQS / FSPIWP | free |
| 21 | 5Vin | | 5 V **input** through D3; output only with "IN-OUT" bridged |
| 22 | GND | | |

| J2 pin | Signal | Other functions (vendor pinout) | Notes |
|---|---|---|---|
| 1 | GND | | |
| 2 | TX = GPIO43 | U0TXD, CLK_OUT1 | **console** (to CH343) |
| 3 | RX = GPIO44 | U0RXD, CLK_OUT2 | **console** (from CH343) |
| 4 | GPIO1 | ADC1_0, TOUCH1, RTC | free |
| 5 | GPIO2 | ADC1_1, TOUCH2, RTC | free |
| 6 | GPIO42 | MTMS (JTAG) | free |
| 7 | GPIO41 | MTDI, CLK_OUT1 | free |
| 8 | GPIO40 | MTDO, CLK_OUT2 | free |
| 9 | GPIO39 | MTCK, CLK_OUT3, SUBSPICS1 | free |
| 10 | GPIO38 | FSPIWP, SUBSPIWP | free |
| 11 | GPIO37 | SPIDQS, FSPIQ | **reserved: octal PSRAM** |
| 12 | GPIO36 | SPIIO7, FSPICLK | **reserved: octal PSRAM** |
| 13 | GPIO35 | SPIIO6, FSPID | **reserved: octal PSRAM** |
| 14 | GPIO0 | BOOT | **BOOT key**, strapping (boot mode) |
| 15 | GPIO45 | (vendor label "VSPI") | **strapping** (VDD_SPI voltage); internal pull-down — must be low at reset |
| 16 | GPIO48 | SPICLK_N | **WS2812** (through the "RGB" link) |
| 17 | GPIO47 | SPICLK_P | free (vendor image says "RGB_LED" — wrong for V1.4, see §8) |
| 18 | GPIO21 | RTC | free |
| 19 | GPIO20 | USB_D+, U1CTS, ADC2_9 | **USB host** — shared with the USB2 connector |
| 20 | GPIO19 | USB_D−, U1RTS, ADC2_8 | **USB host** — shared with the USB2 connector |
| 21 | GND | | |
| 22 | GND | | |

"Free" means the firmware does not touch it and the schematic connects
nothing else to it. GPIO47 and GPIO48 are ordinary 3.3 V pins on R8
modules (only R16V modules run them at 1.8 V — datasheet).

### 3.3 Strapping pins (datasheet)

| Pin | Controls | Default at reset | On this board |
|---|---|---|---|
| GPIO0 | boot mode: 1 = SPI boot, 0 (with GPIO46 = 0) = joint download boot (UART0 or USB) | internal pull-up (1) | BOOT key to GND; CH343 auto-download via Q2 |
| GPIO3 | JTAG signal source, only when eFuse `STRAP_JTAG_SEL` is set (otherwise ignored) | floating | 10 kΩ pull-up only if the "USB-JTAG" link is fitted |
| GPIO45 | VDD_SPI voltage when `EFUSE_VDD_SPI_FORCE` = 0: 0 → 3.3 V, 1 → 1.8 V | internal pull-down (0) | free on J2-15; keep it low at reset (the module's flash/PSRAM are 3.3 V) |
| GPIO46 | boot mode (with GPIO0), ROM log printing | internal pull-down (0) | free on J1-14 |

## 4. Subsystems

### 4.1 Power

```
USB1 "COM"  VBUS ──(VBUSB)──┬── CH343P VDD5
                            └─▶|─ D1 ──┐
USB2 "USB"  VBUS ──┬──▶|─ D2 ─────────┼── 5V rail ── CJ6107A33 LDO ── 3V3 (VDD33)
                   └── "USB-OTG" link ─┘      │
J1-21 5Vin ───────┬──▶|─ D3 ───────────┘      └── WS2812 VDD
                  └── "IN-OUT" link ─────────(J1-21 ↔ 5V)
```

(Drawn from schematic V1.4.)

* Either USB-C port or J1-21 can power the board; the diodes keep the
  sources apart.
* **VBUS to the piano**: the S3's native port is a host only if it can
  supply VBUS. With the "USB-OTG" link **open** (vendor default, by
  inference: the product sheet does not say), D2 only lets that port power
  the board, so a device plugged in gets no VBUS. With the link
  **bridged**, the 5V rail (fed from the COM port through D1) drives USB2
  VBUS directly and the piano enumerates. The bench unit has it bridged.
  The product sheet's underside photo shows a "USB-OTG" marking, which is
  likely where the link is; not confirmed on the bench.
* Power the board from the **COM** port (PC) while the piano is on the
  USB port. With the link bridged, do not connect a PC's powered port to
  the USB connector as well: its VBUS would be tied to the board's 5V rail
  (inference from the schematic, not tested).
* The piano declares itself bus-powered, 100 mA (doc/GZUT_piano_protocol.md),
  but has its own battery. The LDO's current limit and the total budget
  are unknown (no datasheet for CJ6107A33 in the vendor files).
* The USB2 connector has 5.1 kΩ **pull-downs** (Rd, sink) on CC1/CC2,
  not the pull-ups (Rp) a USB-C host normally presents. VBUS does not
  depend on CC here (the link feeds it directly). Which cable the bench
  used (C-to-C or an adapter) is not recorded.
* "IN-OUT" link: bridged → 5 V also comes out on J1-21 for an expansion
  board (vendor NOTE image). Not used.

### 4.2 USB OTG full-speed host (the piano)

* **Bring-up** (`components/yds3_bsp/yds3_bsp.c`, `yds3_usb_host_start`):
  `usb_host_install` with `skip_phy_setup = false` (internal full-speed
  PHY on GPIO19/20), `intr_flags = ESP_INTR_FLAG_LEVEL1`, then a task
  `usb-lib` (4 KB stack, priority 10, pinned to core 0) running
  `usb_host_lib_handle_events` forever, freeing all devices when the last
  client goes. No VBUS switch: VBUS is always on while the link is bridged
  (`init_power` is empty).
* Called from `usb_midi::start` → `board::usb_host_start()` (the HAS=usb
  contract item). Requires ESP-IDF's `usb` component (`REQUIRES usb
  esp_driver_gpio` in the component's CMakeLists).
* **MIDI class client** (`src/usb_midi.rs`, thread `usb-midi`, 8 KB): on
  NEW_DEV it opens the device, parses the configuration descriptor for the
  first Audio / MIDI-Streaming interface (class 1, subclass 3) with bulk
  or interrupt IN/OUT endpoints, claims it, keeps one 64-byte IN transfer
  queued, and sends USB-MIDI event packets on OUT: at most **8 events per
  transfer, 12 ms apart** while a backlog exists (the piano loses events
  from larger bursts). Non-MIDI devices are logged and ignored.
* **The piano**: `GZU-tek CHMidi-2.3`, VID:PID **5022:0716**, USB 2.0
  full speed; interface #1 MIDI Streaming, EP 0x02 OUT / EP 0x82 IN, bulk
  64 B (doc/GZUT_piano_protocol.md). The log shows
  `usb-midi: CHMidi-2.3 (5022:0716) iface … IN … OUT …`; the status name
  becomes "CHMidi-2.3 (USB)".
* **Stale queue at attach**: the piano queues key presses on its USB port
  while no host reads it (board resetting, flashing, booting, or the other
  link in use) and delivers them all within ~30 ms of a host starting to
  read. On this board that once meant five F1 and an E1, i.e. six song
  changes, right after flashing. Since 7894ab8, Note On/Off arriving in
  the first **500 ms** after attach (`STALE_MS`) are dropped and counted
  ("dropped N key events the piano queued before we connected"); panel
  SysEx (`F0 52 4B pp vv 00 00 F7`) still passes. A real press in that
  half-second is lost too.
* Hot-plug works both ways; DEV_GONE tears the device down once no
  transfer is in flight.

### 4.3 UART console (CH343P)

* `CONFIG_ESP_CONSOLE_UART_DEFAULT=y`, `CONFIG_ESP_CONSOLE_SECONDARY_NONE=y`:
  logs go to UART0 (GPIO43/44) and out through the CH343 on the COM port.
  Baud: the ESP-IDF default, 115200 (not overridden).
* There is **no USB Serial/JTAG console**: that controller shares the
  native USB PHY, which belongs to the OTG host.
* The CH343P is powered from the COM port's VBUS; with only the USB port
  or 5Vin powering the board, there is no console.
* DTR / RTS drive EN and GPIO0 through Q1/Q2 (schematic "Auto program"
  table, line levels):

  | DTR | RTS | EN | GPIO0 | Effect |
  |---|---|---|---|---|
  | 1 | 1 | 1 | 1 | run |
  | 0 | 0 | 1 | 1 | run |
  | 1 | 0 | 0 | 1 | **held in reset** |
  | 0 | 1 | 1 | 0 | GPIO0 low (download mode if EN rises now) |

  The table also has an "IO2" column; IO2 is not wired to this circuit in
  the V1.4 schematic (leftover from an ESP32 design). In pyserial,
  `rts=True` drives RTS low: with DTR released that is the "held in
  reset" row.

### 4.4 RGB LED (WS2812B on GPIO48)

* `yds3_led_init` creates an `espressif/led_strip` (~3.0) RMT device:
  GPIO48, 1 LED, `LED_MODEL_WS2812`, GRB, 10 MHz; it is cleared at init.
  `yds3_led_set(r, g, b)` sets and refreshes it. Rust: `status_led(on,
  [r, g, b])` (headless contract item).
* Colours the firmware uses (`board/yds3/mod.rs`, `src/headless.rs`):

  | State | RGB | Look |
  |---|---|---|
  | booting (`early_init`) | 8, 8, 8 | dim white |
  | piano not connected | 24, 10, 0 toggling every 400 ms | blinking amber |
  | piano on USB | 0, 24, 0 | green |
  | piano on Bluetooth | 0, 0, 32 | blue |
  | mode flash after BOOT hold, 1.2 s: Learn | 40, 40, 40 | white |
  | Follow | 40, 30, 0 | yellow |
  | Listen | 30, 0, 40 | violet |
  | `power_off()` | off | |

  The LED is written only when the colour changes. Values are kept low
  on purpose (the WS2812 is bright).

### 4.5 BOOT key

`board/key.rs` turns GPIO0 (active low, internal pull-up) into gestures:
25 ms debounce, a tap is reported once a 280 ms double-tap window has
passed, a hold after 600 ms. `src/headless.rs` maps them:

| Gesture | Action |
|---|---|
| tap | play / pause (restart if stopped or finished) |
| double tap | next song in the folder (starts at once) |
| hold (≥ 600 ms) | next mode Learn → Follow → Listen, with the LED flash above |

The piano's lowest and highest keys are a remote control as on every
board (`piano_core::controls`); C8 (bookmark) only logs "bookmarks are not
kept on this board".

`power_off()` (wait for BOOT release, LED off, `esp_sleep_enable_ext0_wakeup(GPIO0, 0)`,
deep sleep; BOOT or RST wakes) exists for the contract, but the headless
UI never calls it. `reboot()` is likewise unused (full UI only).

### 4.6 Wi-Fi access point and web page

`src/web.rs`, `web/index.html` (gzipped by build.rs):

* Soft-AP **`PianoCoach-XXXX`** (XXXX = last two bytes of the soft-AP MAC
  in upper-case hex), WPA2-Personal, password **`pianocoach`**, channel 6,
  at most 4 clients, address **192.168.4.1/24** with DHCP, the board as
  DNS server. The bench unit's SSID is not recorded in the repo.
* A tiny DNS server (thread `dns`, UDP 53) answers every A query with
  192.168.4.1, and unknown URLs get a 302 to `http://192.168.4.1/`, so
  phones show the page as a captive portal.
* HTTP: `GET /` (page), `/api/state`, `/api/dir?p=`, `/api/search?q=`,
  `/api/play?p=`, `/api/cmd?c=…&v=…` (playpause, stop, restart, next,
  prev, mode, hands, tempo, vol, sound, ledtest, alloff). Paths with `..`
  are refused. Server stack 10 KB, 8 URI handlers, started **before**
  Wi-Fi.
* `/api/state` reports `"speaker"` false here, so the page hides the
  sound-source and volume rows.
* Coexistence with BLE: `CONFIG_ESP_COEX_SW_COEXIST_ENABLE=y`, radio
  sharing left "balanced" (preferring BT made page loads take > 8 s). See
  §4.7 for the BLE link parameters that make this work.
* Testing from the PC: join the AP with the USB Wi-Fi dongle
  `wlxa0d768003aa5` (`nmcli dev wifi connect PianoCoach-XXXX password
  pianocoach ifname wlxa0d768003aa5`), set that profile to
  `autoconnect no` / `ipv4.never-default yes`, and reconnect the home
  network afterwards (AGENTS.md).

### 4.7 Bluetooth LE and the dual transport

* Native BLE controller + NimBLE host (central + observer only, one
  connection, MTU 260, bonds in NVS, security off, NimBLE memory in
  PSRAM). `bt_controller_start` is empty (no coprocessor).
* `ble_midi.rs` scans for `GZUT-MIDI` (or the MIDI service UUID),
  connects, and writes `80 80` + plain MIDI (the piano's BLE quirk).
* Headless boards are built with `cfg(ble_relaxed)` (build.rs): the
  connection asks for a **30–50 ms interval (24–40 × 1.25 ms), latency 0,
  10 s supervision timeout** (`SUPERVISION_10MS = 1000`), and when the
  piano renegotiates to 10 ms / 500 ms after connecting, `relax_link`
  asks again (at most twice per connection). Writes are gathered for
  15 ms (`WRITE_GAP`) into one packet.
* **`src/dual.rs`** (compiled for `all(has_usb, ui_headless)`, i.e. only
  this board today): starts the USB client (if the host stack fails to
  install it logs "Bluetooth only") and the BLE central, plus a router
  thread `midi-route` (4 KB):
  * outgoing messages go to USB while the piano is on the cable, else to
    BLE; the switch happens on the fly, mid-song (the piano keeps its lit
    keys);
  * key events from USB always pass; key events from BLE are **dropped
    while USB is up** (the piano reports each key on both links);
  * the combined status: connected if either link is up, `usb` flag for
    the LED colour, name with the link ("CHMidi-2.3 (USB)"), panel SysEx
    from the active link.
* The BLE connection stays up while USB carries the traffic, and the
  piano accepts one BLE central at a time: while this board is powered,
  a tablet or another board cannot connect over BLE.

### 4.8 Song pack in flash

* No microSD. `main.rs` calls `songs::open_pack()` on headless boards
  without `HAS=sd`: it finds the data partition labelled `songs`, checks
  the header and index CRC and **memory-maps** the partition (no RAM for
  the index; `CONFIG_SPI_FLASH_MMAP_PAGES_PRESERVED=n` leaves room for an
  ~11 MB mapping). Songs are inflated one at a time on load.
* `browse.rs` shows the pack as a folder tree under a virtual `/songs`
  root; a song's path maps to `/pack/<index>`. The search index is built
  from the pack in the background (thread `index`, 16 KB): 2449 songs in
  1.4 s.
* `SONGPACK_ARGS="--max-bytes 11.9M"`: the whole JustPiano library
  (2449 songs) fits. The pack built on 2026-10-16
  (`target/yds3/songs.bin`) is 9,347,030 bytes.
* Flashing the app does not touch the pack; `tools/songs.sh yds3 flash`
  writes it at the `songs` offset (0x410000). Without a pack the log says
  "no song pack in flash — tools/songs.sh <board> build, then flash".
* Boot: Follow mode and a random song from 儿时回忆, started as soon as
  the piano connects (either link); the last song played is a fallback
  (pref `lastsong`).

## 5. Memory map and partitions

`boards/yds3/partitions.csv` (16 MB, no OTA). The bootloader is at 0x0
and the partition table at 0x8000 (ESP-IDF defaults).

| Name | Type / subtype | Offset | Size | Contents |
|---|---|---|---|---|
| nvs | data / nvs | 0x9000 | 0x6000 (24 KB) | prefs (namespace `piano`, i32 values: `mode`, `hands`, `tempo`, `sound`, …; `lastsong` string), NimBLE bonds |
| phy_init | data / phy | 0xF000 | 0x1000 (4 KB) | RF calibration |
| factory | app / factory | 0x10000 | 0x400000 (4 MB) | firmware (~2.5 MB used) |
| songs | data / undefined | 0x410000 | 0xBF0000 (12,517,376 B ≈ 11.9 MB) | song pack, written by `tools/songs.sh` |

Notable `sdkconfig.defaults` settings:

| Setting | Why |
|---|---|
| `ESPTOOLPY_FLASHMODE_QIO`, `FLASHSIZE_16MB`, custom partition table | module flash; the 11.9 MB song partition |
| `ESP_CONSOLE_UART_DEFAULT`, `ESP_CONSOLE_SECONDARY_NONE` | console via CH343; the native USB PHY is the OTG host's |
| `FREERTOS_HZ=1000` | ms timing for MIDI; the USB client polls with a 1-tick (1 ms) timeout |
| `ESP_MAIN_TASK_STACK_SIZE=8192`, `PTHREAD_TASK_STACK_SIZE_DEFAULT=8192`, `ESP_TASK_WDT_TIMEOUT_S=15` | no reason given in the file (the same values as the other Xtensa boards; a 4 KB main-task stack overflowed on the WROVER-KIT) |
| `SPIRAM`, `SPIRAM_MODE_OCT`, `SPIRAM_SPEED_80M`, `SPIRAM_USE_MALLOC` | 8 MB octal PSRAM for songs, the engine timeline, the search index |
| `SPIRAM_MALLOC_ALWAYSINTERNAL=0`, `SPIRAM_MALLOC_RESERVE_INTERNAL=32768`, `SPIRAM_TRY_ALLOCATE_WIFI_LWIP` | every plain malloc goes to PSRAM. At the default 4096, the search index's thousands of small strings filled internal RAM and Wi-Fi start-up panicked. Now 89 KB internal stays free with Wi-Fi, BLE and USB host running; drivers still ask for internal/DMA memory explicitly |
| `SPI_FLASH_MMAP_PAGES_PRESERVED=n` | the song pack is memory-mapped (~11 MB) |
| `ESP_COEX_SW_COEXIST_ENABLE`, `HTTPD_MAX_REQ_HDR_LEN=1024`, `HTTPD_MAX_URI_LEN=1024` | Wi-Fi AP beside BLE; the larger HTTP limits are not explained in the file (the API carries URL-encoded Chinese paths in query strings) |
| `BT_NIMBLE_*` (central + observer, 1 connection, MTU 260, NVS persist, security off, memory external, host stack 8 KB, log WARNING) | BLE-MIDI central for the piano |
| `FATFS_LFN_HEAP`, `FATFS_MAX_LFN=255`, `FATFS_USE_LABEL` | no SD card, but the shared ESP-IDF checkout has the exFAT patch, which does not compile FatFs without LFN |

Threads (stack): `app` 24 KB, `ui` (headless loop) 10 KB, `usb-midi`
8 KB, `usb-lib` 4 KB (C task, core 0), `midi-route` 4 KB, NimBLE host
8 KB, HTTP server 10 KB, `dns` 4 KB, `index` 16 KB, player (see
`player.rs`).

## 6. Build, flash, console

All from `firmware/piano-coach/`. Do not run builds or flash without
the user at the bench.

```bash
tools/board.sh yds3 build --release                  # first build compiles ESP-IDF (10+ min)
tools/songs.sh yds3 build                            # target/yds3/songs.bin (no glyphs: headless)
tools/songs.sh yds3 flash --port /dev/ttyACMx --baud 921600   # pack at 0x410000
tools/board.sh yds3 run --release -- --port /dev/ttyACMx      # flash app + monitor (blocks)
```

* `tools/board.sh` sources `boards/yds3/board.env` and runs `cargo
  +esp198 … --target xtensa-esp32s3-espidf --no-default-features
  --features board-yds3`, with `target/yds3` as the target dir and
  `sdkconfig.yds3` as the generated sdkconfig. `cargo run` uses
  `flash.sh`, i.e. `espflash flash --monitor` with the build's own
  bootloader and partition table.
* Direct flash without monitor (T = `target/yds3/xtensa-esp32s3-espidf/release`;
  `bootloader.bin` / `partition-table.bin` are in `T` or `T/build`):
  `espflash flash --chip esp32s3 --port /dev/ttyACMx --baud 921600 --bootloader T/bootloader.bin --partition-table T/partition-table.bin T/piano-coach`
* **Which port**: always the **COM** connector (CH343). Find it with
  `udevadm info -q property -n /dev/ttyACMx | grep -E 'ID_VENDOR_ID|ID_MODEL_ID|ID_SERIAL'`
  (1a86 / 55d3) or `ls -l /dev/serial/by-id/`; `espflash board-info --port …`
  shows the chip and MAC (and resets the board). Port numbers change on
  re-plug (seen as `/dev/ttyACM2` in the firmware doc).
* **Download mode**: automatic through DTR/RTS (espflash). By hand: hold
  BOOT, press and release RST, release BOOT. The native USB port in ROM
  download mode would also offer USB-Serial/JTAG download (datasheet);
  not tried on this board.
* **Reading logs without resetting**: open the port with DTR and RTS
  released, e.g. pyserial `s = serial.Serial(); s.port = P; s.baudrate =
  115200; s.dtr = False; s.rts = False; s.open()`. Reset with an RTS
  pulse (`s.rts = True; time.sleep(0.1); s.rts = False`). **Holding
  `rts=True` keeps the board in reset**, which is how the stale-USB-queue
  test held it for 20 s while keys were pressed on the piano.
* Do not `pkill -f` a pattern that matches your own shell; use PID files.
* Setting a pref from the PC: build an NVS image with ESP-IDF's
  `nvs_partition_gen.py generate prefs.csv nvs.bin 0x6000` and write it
  with `espflash write-bin --port P 0x9000 nvs.bin`. This replaces all
  prefs and BLE bonds.

## 7. Piano Coach support

UI class **headless** (`src/headless.rs`, `src/web.rs`): BOOT key, RGB
LED, phone web page, piano-key remote; no speaker, so no tones and no
sound-source / volume rows.

Verified on hardware (bench unit V1.4, ESP32-S3 rev 0.2), 2026-10-16:

| Check | Result |
|---|---|
| Piano on the USB-C OTG port (`CHMidi-2.3`, 5022:0716) and over BLE at once | ✔ |
| Learn on 小星星 over USB | ✔ |
| Unplug USB mid-song → switches to Bluetooth, song carries on; replug → back to USB | ✔ |
| BOOT tap / double tap / hold, RGB LED colours | ✔ |
| Web page, folder list, pinyin search from a PC (song pack 2449 songs) | ✔ |
| App size | 2.5 MB of 4 MB |
| Internal RAM free with Wi-Fi + BLE + USB host | 89 KB |
| Stale USB queue (7894ab8): board held in reset 20 s via RTS while F1 was pressed; 14, then 27 queued events dropped at attach, no song change, boot song started; a live F1 afterwards changed category | ✔ |

Not verified on this board:

* The phone captive-portal flow (verified on the Audio Kit only; here
  the page was used from a PC).
* The full piano-key remote list and shuffle-play (only F1 was tried
  here); shuffle's automatic next song is untried on all small and
  headless boards.
* A long two-way BLE-only session (done on the 1732S019, another S3).
* `power_off()` / deep-sleep wake (not reachable from the headless UI).

Limits:

* No bookmarks (C8 does nothing), no A–B loop, no sheet view, no speaker.
* The BLE link is held even while USB is in use, so other BLE centrals
  cannot reach the piano while the board is on.
* A real key press in the first 500 ms after USB attach is dropped.

## 8. Quirks and lessons learned

* **"USB-OTG" jumper**: without it the piano gets no VBUS and never
  enumerates on the native port. It bypasses D2, which otherwise only
  lets that port power the board.
* **GPIO47 vs GPIO48**: the vendor pinout image (`yd-esp32-s3_pinout.jpg`)
  labels GPIO47 "RGB_LED". The V1.4 schematic connects the WS2812 to
  GPIO48 (through the "RGB" link), and the firmware uses GPIO48 with the
  colours verified. Code wins; other board revisions may differ.
* **Console only on the COM port**: the native USB port belongs to the
  OTG host, so a USB Serial/JTAG console is impossible while the firmware
  runs.
* **The piano's USB key queue**: whenever no host reads the piano's USB
  port, presses pile up and arrive in a burst at attach; hence `STALE_MS`.
  The same burst would otherwise jump through songs after every flash.
* **Double key reports**: with both links up the piano sends each key on
  USB and BLE; `dual.rs` drops the BLE copy while USB is up.
* **Internal RAM**: `SPIRAM_MALLOC_ALWAYSINTERNAL=0` is required. At 4096
  Wi-Fi start-up panicked once the search index had been built.
* **GPIO35–37 are on the header but belong to the octal PSRAM.**
* **CH343 on Linux** is `/dev/ttyACM*` (cdc_acm), not `/dev/ttyUSB*`.
* Vendor boot-log screenshots (`yd-esp32-s3_bootlog-*.{jpg,png}`) show
  the vendor's demo built with DIO flash and 40 MHz PSRAM; Piano Coach
  uses QIO and 80 MHz. The screenshots confirm 16 MB flash
  (16777216 bytes), chip rev v0.2 and 8 MB "generation 3" AP octal PSRAM
  on the vendor's sample.
* **Repo docs vs code**: `doc/piano_coach_firmware.md` and AGENTS.md give
  the relaxed BLE link as "30–50 ms / 4 s", but `ble_midi.rs` asks for a
  **10 s** supervision timeout (`SUPERVISION_10MS = 1000`). The firmware
  doc says BOOT hold is "~1 s", but `key.rs` reports a hold at **600 ms**.
  This document follows the code.
* The vendor product sheet's subtitle ("N8R2/N168R … USB Micro Type-C")
  has typos: this board has USB-C and an N16R8 module. The NOTE image
  shows an N8R2 unit of the same PCB.

## 9. Sources

In the repo:

* `firmware/piano-coach/boards/yds3/board.env`, `sdkconfig.defaults`,
  `partitions.csv`, `esp-idf-components.toml`
* `firmware/piano-coach/src/board/yds3/mod.rs`, `src/board/mod.rs`,
  `src/board/contract.rs`, `src/board/key.rs`
* `firmware/piano-coach/components/yds3_bsp/` (`yds3_bsp.c`,
  `include/yds3_bsp.h`, `CMakeLists.txt`, `idf_component.yml`)
* `firmware/piano-coach/src/dual.rs`, `src/usb_midi.rs`,
  `src/ble_midi.rs`, `src/headless.rs`, `src/web.rs`, `src/main.rs`,
  `src/songs.rs`, `src/link.rs`, `src/bindings.h`, `build.rs`, `flash.sh`,
  `tools/board.sh`, `tools/songs.sh`, `.cargo/config.toml`
* `AGENTS.md`, `doc/piano_coach_firmware.md`, `doc/GZUT_piano_protocol.md`,
  `firmware/piano-coach/boards/README.md`, commits aed283e and 7894ab8
* Vendor files copied to `doc/boards/vendor/yds3/`:
  * `yd-esp32-s3_schematic-v1.4.pdf`: schematic "YD-ESP32-S3-COREBOARD
    V1.4" (was `YD-ESP32-S3-SCH-V1.4.pdf`)
  * `yd-esp32-s3_pinout.jpg`: pin definitions (was `1.jpg`)
  * `yd-esp32-s3_hardware-overview.jpg`: labelled board photo (was `2.jpg`)
  * `yd-esp32-s3_jumper-note.jpg`: "RGB" and "IN-OUT" link note (was `NOTE.jpg`)
  * `yd-esp32-s3_bootlog-16mb-flash.jpg`, `yd-esp32-s3_bootlog-8mb-psram.png`:
    vendor boot logs (were `16MB Flash.jpg`, `8MB PSRAM.png`)
  * `yd-esp32-s3_dimensions-mm.pdf`: board drawing in mm (was `ESP32-S3-Metric.pdf`)

The vendor's 5-page product sheet (`/home/nws/w/esp32/doc/ESP32-S3/S3faa6404fc614658a153b712d066e8fdA.pdf`,
9.9 MB) repeats the pinout, photo and schematic above and is not copied;
nor are its tool screenshots (`3.jpg`).

Outside the repo (generic Espressif datasheets, not copied):
`/home/nws/w/esp32/doc/ESP32-S3/24011015-ESP32-S3 Dev/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf`
(v1.3: module variants, IO35–37 octal PSRAM note, strapping pins) and
`esp32-s3_datasheet_en.pdf` (v1.8: in-package flash/PSRAM pin mapping,
GPIO26–37). The vendor folder also holds a MicroPython firmware and
example scripts, Thonny, a CP2102 note, the inch drawing and a video,
none of which this document uses.
