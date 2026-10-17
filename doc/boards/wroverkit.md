# ESP-WROVER-KIT v4.1 (`wroverkit`)

Hardware reference for Piano Coach on Espressif's ESP-WROVER-KIT v4.1.
Facts the firmware relies on come from the code and are verified on the
bench unit; facts that only come from the vendor schematic are marked
"(schematic, not used by firmware)". The vendor schematic is in
[vendor/wroverkit/ESP-WROVER-KIT_V4_1.pdf](vendor/wroverkit/ESP-WROVER-KIT_V4_1.pdf).

## 1. Summary

The ESP-WROVER-KIT is Espressif's development board for the original
ESP32: an ESP32-WROVER-family module (dual Xtensa LX6 at 240 MHz, flash
and PSRAM in the module), a 3.2″ 320×240 SPI LCD, a microSD slot, an RGB
LED, a camera header, BOOT and EN buttons and an FTDI FT2232HL that gives
the PC both a JTAG adapter (channel A) and the UART0 console (channel B)
over one micro-USB cable. Piano Coach runs on it with the **minimal UI**
drawn at 1.25×, reaches the piano over **BLE only**, and keeps **776
songs in a flash song pack**. The microSD slot is not used (its
card-detect shares the LCD's D/C pin) and the module's PSRAM is left off
(the bench unit's chip is revision 1.0).

| Item | Value |
|---|---|
| Vendor / product | Espressif ESP-WROVER-KIT, schematic title `ESP32-WROVER-KIT_V4.1`, Rev 1.0, dated 2018-08-29 |
| Board dir | `firmware/piano-coach/boards/wroverkit/`, Rust module `firmware/piano-coach/src/board/wroverkit/mod.rs` |
| Cargo feature | `board-wroverkit` |
| `CAPS.name` | `ESP-WROVER-KIT v4.1` |
| MCU | ESP32, **chip revision 1.0** on the bench unit (hardware log in `doc/piano_coach_firmware.md`) |
| Module | ESP32-WROVER family (code: "ESP32-WROVER"). Exact variant **unknown**: the ESP32-WROVER-E carries an ESP32-D0WD-V3 (revision v3.x per the ESP32 datasheet), so a rev 1.0 chip means an older (pre-"E") WROVER. The WROVER-E datasheet was used here only for the 38-pin layout and pin functions, which the schematic shows as the same footprint ("Support ESP32 Module: compatible old and new module"). |
| CPU / arch | dual-core Xtensa LX6, 240 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240=y`) |
| Rust target / toolchain | `xtensa-esp32-espidf`, esp-rs toolchain `esp198` (`espup install --name esp198 --toolchain-version 1.98.1.0`) |
| ESP-IDF | v5.5.3 (`ESP_IDF_VERSION` in `board.env`) |
| Flash | 4 MB, DIO (`CONFIG_ESPTOOLPY_FLASHSIZE_4MB`, `CONFIG_ESPTOOLPY_FLASHMODE_DIO`) |
| PSRAM | Present in the module, **disabled** (`CONFIG_SPIRAM=n`): ESP32 rev 1 needs a cache workaround compiled into every piece of code that touches PSRAM, and the Rust code is not built with it. Size unknown (not probed). RAM is therefore the internal SRAM only (~145 KB heap free when running). |
| Display | 3.2″ 320×240 SPI LCD, ILI9341 or ST7789V (auto-detected; the bench unit has an ILI9341) |
| USB | FT2232HL dual USB-UART/JTAG. VID:PID **not confirmed** on this unit (FTDI's default for the FT2232H is 0403:6010; the board has a 93C46 EEPROM, U3, that can hold another ID — check `lsusb`). Interface 0 / channel A = JTAG (`/dev/ttyUSB0` when nothing else is attached), interface 1 / channel B = UART0 console and flashing (`/dev/ttyUSB1`). |

## 2. Feature overview

| Component | Part (schematic ref) | Function | Bus / pins | Used by Piano Coach |
|---|---|---|---|---|
| Module | ESP32-WROVER family (D1 / U2: two footprints for old and new modules) | MCU, flash, PSRAM | — | yes (PSRAM off) |
| LCD | 3.2″ 320×240, 18-pin connector U5; controller ILI9341 or ST7789V | display | SPI: SCL 19, SDA (MOSI) 23, SDO (MISO) 25, CS 22, D/C 21, RESET 18 | yes |
| LCD backlight | Q5 + Q4 (S8050 NPN), R126 15 Ω to LEDK | backlight switch | GPIO5, active low | yes (LEDC PWM) |
| LCD touch pins | U5 pins 15–18 (XL, YU, XR, YD) | resistive touch | not connected (marked NC) | no (no touch) |
| USB bridge | FT2232HL (U1), 12 MHz crystal XO1, 93C46 EEPROM (U3) | channel A JTAG, channel B UART0 | micro-USB J5 | yes (console, flashing) |
| Auto-program | Q1, Q2, Q9 (S8050) | DTR/RTS → EN, IO0, IO2 | — | yes (espflash resets) |
| UART LEDs | D6 (blue + red) | FT2232 RX/TX activity (nRXLED / nTXLED) | FT2232 BCBUS | n/a (bridge hardware) |
| FT2232 GPIO LEDs | D9–D12 red, test points TP1–TP4 | FT_GPIO0–3 (FT2232 ACBUS4–7) | — | no (schematic, not used by firmware) |
| BOOT button | SW3 "SWITCH BUTTON", via Q10 / Q11 (LBSS138) | pulls GPIO0 **and GPIO2** low | GPIO0, GPIO2 | yes (GPIO0 as the user key) |
| EN button | SW4 | resets the chip (EN to GND) | EN | yes (reset / wake) |
| RGB LED | 5050RGB (U6), low-side NMOS Q6/Q7/Q8 (LBSS138), 5.1 kΩ each | three LEDs | GPIO2 (Q6), GPIO0 (Q7), GPIO4 (Q8), active high | no (schematic, not used by firmware) |
| microSD | TF_CARD (J4), 10 kΩ pull-ups R152–R156, R157 NC | 4-bit SD + card detect | D0 2, D1 4, D2 12, D3 13, CLK 14, CMD 15, CD 21 | no — must stay empty (see 4.6) |
| Camera header | JP4, 2×9 female | parallel (DVP) camera | see 4.7 | no (schematic, not used by firmware) |
| JTAG / UART jumpers | JP2, 2×14 | connects FT2232 to UART0 and to MTDI/MTCK/MTMS/MTDO | see 4.8 | UART jumpers needed for the console; JTAG unused |
| UART flow-control jumper | JP14, 2×2 | IO13 ↔ nRTS, IO15 ↔ nCTS | GPIO13, GPIO15 | no (schematic, not used by firmware) |
| I/O expansion header | JP1, 2×14 male | GPIO break-out, VDD33, GND, SW_+5V, EN | see 3 | no |
| 32.768 kHz crystal | XO2, C12/C13 12 pF | RTC slow clock option | GPIO32 / GPIO33 (via R11 / R23 0 Ω) | no (firmware keeps the default RTC clock) |
| Power input select | JP7, 2×2 | +5V from USB_5V or EXT_5V | — | hardware setting |
| Power switch | J3 (CON3) | switches +5V to SW_+5V | — | hardware setting |
| External 5 V | J2, 4-pin header, D13 SS14 | EXT_5V input | — | no |
| 3.3 V regulator | NCP1117 (U4); note: "Second source LM317DCY(1.5A)" | SW_+5V → VDD33 | — | n/a |
| Power LED | D7 red | on SW_+5V | — | n/a |

## 3. Pin map

"Used" = driven or read by Piano Coach. Sources: **code** =
`src/board/wroverkit/mod.rs` (and `spi_lcd.rs`, `key.rs`); **sch** = the
vendor schematic; **dsh** = ESP32-WROVER-E / ESP32 datasheets.

| GPIO | Function on this board | Dir / level | Peripheral | Used | Source | Shared with |
|---|---|---|---|---|---|---|
| 0 | BOOT key (SW3 via Q10) | in, pressed = low, internal pull-up | GPIO; deep-sleep ext0 wake (RTC_GPIO11) | **yes** | code, sch | RGB LED (Q7 gate), camera RESET, auto-program Q2; strapping (boot mode) |
| 1 | U0TXD → FT2232 channel B RXD (JP2 3–4) | out | UART0 | **yes** (console) | sch, sdkconfig | — |
| 2 | — | — | — | no | sch | BOOT key (SW3 via Q11 also pulls it low), RGB LED (Q6), SD DATA0 (10 kΩ pull-up R153), auto-program Q9, JP1; strapping (boot mode) |
| 3 | U0RXD ← FT2232 channel B TXD (JP2 5–6) | in | UART0 | **yes** (console) | sch, sdkconfig | — |
| 4 | — | — | — | no | sch | RGB LED (Q8), SD DATA1 (R152 pull-up), camera D0, JP1 |
| 5 | LCD backlight | out, **on = low** | LEDC PWM, 20 kHz, 10-bit, output inverted | **yes** | code, sch | camera D1, JP1; strapping (SDIO timing) |
| 6–11 | module SPI flash | — | — | (flash) | dsh | not led out on WROVER-E (pins 17–22 NC); the schematic routes SD0–SD3/CLK/CMD to JP2 9–14 for older modules |
| 12 | (MTDI) | — | — | no | sch | SD DATA2 (pull-up R157 **NC**), JTAG TDI (pull-up R54 NC), JP1; strapping: VDD_SDIO voltage — keep it low at reset |
| 13 | (MTCK) | — | — | no | sch | SD DATA3 (R156 pull-up), JTAG TCK (R50 10 kΩ pull-up), JP14 → nRTS, JP1 |
| 14 | (MTMS) | — | — | no | sch | SD CLK (R154 pull-up), JTAG TMS (R55 10 kΩ pull-up), JP1 |
| 15 | (MTDO) | — | — | no | sch | SD CMD (R155 pull-up), JTAG TDO (R69 NC), JP14 → nCTS, JP1; strapping |
| 16, 17 | — | — | — | no | sch, dsh | JP1 23 / 22 via R147 / R146 (0 Ω). On WROVER modules these carry the in-module PSRAM (ESP32 datasheet: GPIO16 = PSRAM CE#, GPIO17 = PSRAM SCLK); the WROVER-E datasheet lists module pins 27 / 28 as NC. Do not use. |
| 18 | LCD RESET | out, reset = low | GPIO | **yes** | code, sch | camera D2, JP1 |
| 19 | LCD SCL (SPI clock) | out | SPI2_HOST (HSPI) via GPIO matrix | **yes** | code, sch | camera D3, JP1 |
| 21 | LCD D/C | out | SPI2 (esp_lcd D/C) | **yes** | code, sch | **microSD card detect** (SD_DET via R167, 10 kΩ pull-up R168), camera XCLK, JP1 |
| 22 | LCD CS | out, active low | SPI2 | **yes** | code, sch | camera PCLK, JP1 |
| 23 | LCD SDA (MOSI) | out | SPI2 | **yes** | code, sch | camera HREF, JP1 |
| 25 | LCD SDO (MISO) | in | SPI2 (panel ID read only) | **yes** | code, sch | camera VSYNC, JP1 |
| 26 | — | — | — | no | sch | camera SIO_D (SCCB data), JP1 |
| 27 | — | — | — | no | sch | camera SIO_C (SCCB clock), JP1 |
| 32, 33 | 32.768 kHz crystal XO2 | — | XTAL_32K_P / _N | no | sch, dsh | Crystal fitted (R11 / R23 0 Ω); the path to JP1 3 / 4 is open (R12 / R24 NC), so not usable as GPIO as shipped |
| 34 | — | in only | — | no | sch | camera D6, JP1 |
| 35 | — | in only | — | no | sch | camera D7, JP1 |
| 36 (SENSOR_VP) | — | in only | — | no | sch | camera D4, JP1 14 |
| 39 (SENSOR_VN) | — | in only | — | no | sch | camera D5, JP1 13 |
| EN | reset | in, reset = low | — | (reset) | sch | SW4, auto-program Q1, JP1 15, JP2 15 |

Code and schematic agree on every pin the firmware uses (LCD RST 18,
SCL 19, D/C 21, CS 22, SDA 23, SDO 25, backlight 5, BOOT 0); no
disagreement was found.

**Shared-pin summary.** The LCD and the camera header use the same pins
(18, 19, 21, 22, 23, 25, 5) and are mutually exclusive. The microSD slot
uses the JTAG pins (12–15) and GPIO2/GPIO4 (also the RGB LED), and its
card-detect switch shares GPIO21 with the LCD's D/C line, so microSD and
LCD are mutually exclusive in practice. The RGB LED shares GPIO0/2/4 with
the BOOT key, microSD and camera.

JP1 (I/O expansion, 2×14 male) pin-out (schematic): 1 VDD33, 2 GND,
3 IO32, 4 IO33, 5 IO12, 6 IO13, 7 IO14, 8 IO27, 9 IO26, 10 IO25, 11 IO35,
12 IO34, 13 IO39 (SENSOR_VN), 14 IO36 (SENSOR_VP), 15 EN, 16 IO23, 17 IO22,
18 IO21, 19 IO19, 20 IO18, 21 IO5, 22 IO17, 23 IO16, 24 IO4, 25 IO0,
26 IO2, 27 IO15, 28 SW_+5V. Optional 10 kΩ pull-ups to SW_+5V on every
line are not fitted (NC). (Schematic, not used by firmware.)

## 4. Subsystems

### 4.1 Power

(Schematic, not used by firmware.)

* **Sources**: USB_5V from the micro-USB J5 (through D2 SS14) and EXT_5V
  from the 4-pin header J2 (through D13 SS14). The schematic notes that
  with everything in use (LCD, camera …) USB power may not be enough,
  which is why J2 exists.
* **JP7** (2×2) selects which one becomes +5V: pins 1–3 = EXT_5V,
  pins 2–4 = USB_5V. For USB power, jumper 2–4.
* **J3** (3-pin slide switch) connects +5V to SW_+5V (on); the third pin
  goes to GND. The red LED D7 (2 kΩ) lights when SW_+5V is present.
* **U4 NCP1117** turns SW_+5V into VDD33 (its ADJ pin is tied to GND
  through R124 0 Ω, i.e. the fixed-output part; R125 / C33 for an
  adjustable part are NC). VDD33 feeds the module (via R166 0 Ω as
  ESP_VDD33), the LCD, microSD and camera.

### 4.2 USB / FT2232HL (JTAG + UART, auto-reset)

* FT2232HL (U1), 12 MHz crystal, 93C46 EEPROM, USB on micro-USB J5.
* **Channel A** (ADBUS0–3 = TCK, TDI, TDO, TMS) is the JTAG adapter,
  connected to the ESP32 only through the JP2 jumpers (4.8). On Linux the
  ftdi_sio driver also gives this interface a tty (`/dev/ttyUSB0` when no
  other USB-serial device is present); OpenOCD detaches it when used.
* **Channel B** (BDBUS0–4 = TXD, RXD, nRTS, nCTS, nDTR) is the UART: TXD/
  RXD go to the ESP32's RXD0/TXD0 through JP2. This is the **console and
  flashing port**, `/dev/ttyUSB1`. Firmware console: UART0 (comment in
  `sdkconfig.defaults`: "Console on UART0 (FT2232 channel B →
  /dev/ttyUSB1)").
* Port numbers move when other devices are attached (the bench PC also
  has an FTDI TTL-234X cable, which takes a `ttyUSB` number of its own).
  Identify the console by interface number:
  `udevadm info -q property -n /dev/ttyUSBx | grep -E 'ID_SERIAL|ID_USB_INTERFACE_NUM'`
  — interface `01` is the console, `00` is JTAG.
* **Auto-program** (schematic table "DTR RTS → EN IO0"): Q1 (base nDTR,
  emitter nRTS) drives EN, Q2 and Q9 (base nRTS, emitter nDTR) drive IO0
  and IO2.

  | DTR | RTS | EN | IO0 |
  |---|---|---|---|
  | 1 | 1 | 1 | 1 |
  | 0 | 0 | 1 | 1 |
  | 1 | 0 | 0 | 1 |
  | 0 | 1 | 1 | 0 |

  IO2 follows IO0 (Q9 is wired like Q2), which is what download mode
  needs (GPIO0 = 0 and GPIO2 = 0). espflash uses this; no buttons are
  needed to flash.
* **Opening the port** with DTR/RTS asserted resets the board (or holds
  it in reset); open it with DTR and RTS false (pyserial) to read logs
  without a reset.

### 4.3 Display

* **Panel**: 3.2″, 320×240, connector U5 (GND, RESET, SCL, D/C, CS, SDA,
  SDO, GND, VCC, LEDA, 4× LEDK, XL/YU/XR/YD unconnected). Pins from the
  code and the schematic agree (section 3). VCC = VDD33, LEDA = VDD33.
* **Controller detection** (`lcd_init`): after a hardware reset on GPIO18
  (low 20 ms, high, 120 ms wait; 1 kHz tick) the firmware reads RDDID (0x04, 3
  bytes) over SDO/GPIO25. **All zeros = ILI9341**, anything else =
  ST7789V — the same rule as ESP-IDF's `spi_master/lcd` example. The bench
  unit returns zeros (ILI9341). Log line: `lcd: ID 000000 → ILI9341`.
* **Driver**: both controllers are driven through esp_lcd's ST7789 driver
  (`esp_lcd_new_panel_st7789`; they share the MIPI DCS commands), followed
  by each controller's power/VCOM/gamma table copied from the ESP-IDF
  example (`ILI9341` / `ST7789V` arrays in `mod.rs`). Colour order BGR on
  the ILI9341, RGB on the ST7789V; 16 bpp RGB565, byte-swapped to big
  endian in `SpiLcd::draw`.
* **Orientation**: landscape as in the example — `swap_xy(true)` (MV),
  and `mirror(x = true)` on the ST7789V only. `LCD_MIRROR = None`: the
  orientation is the same on every unit.
* **Bus**: `SPI2_HOST`, mode 0, DMA auto, **26 MHz**, max transfer one
  10-line band (320 × 10 × 2 bytes). Why not faster: the code comment
  says the clock pin goes through the GPIO matrix (26.7 MHz max). The
  module datasheet explains it: the IOMUX VSPI clock pin is GPIO18
  (VSPICLK) and GPIO19 is VSPIQ, but this board wires the LCD clock to
  GPIO19 and RESET to GPIO18, so the clock cannot use the IOMUX path on
  either SPI host.
* **UI scale**: `UI_SCALE = 1.25` — the minimal UI's bar layout is
  designed for 170 rows; this panel has 240, so it is drawn at 1.25×
  (logical 256×192). `ROUND = false`.
* **Backlight**: GPIO5, **active low**. Chain (schematic): GPIO5 → R164
  → Q5 (NPN, collector pulled up by R165 10 kΩ) → R59 → Q4 (NPN) → R126
  15 Ω → LEDK. GPIO5 low → Q5 off → Q4 on → backlight on. The firmware
  uses LEDC timer 0 / channel 0, 20 kHz, 10-bit, with `output_invert`
  (`spi_lcd::backlight_init(5, true)`); `backlight_set` never goes below
  duty 40/1023 except at 0 %. (The generic comment in `spi_lcd.rs`
  calls an active-low backlight "a PNP switch"; here it is two NPN
  stages — same polarity.)

### 4.4 Buttons

* **BOOT (SW3)**: SW3 connects VDD33 to the gates of Q10 and Q11
  (LBSS138, RC C24 1 nF / R170 10 kΩ), which pull **GPIO0 and GPIO2**
  low while pressed. The firmware reads only GPIO0 (`board::key::Key`,
  input with internal pull-up, pressed = low): tap, double tap (280 ms
  window) and hold (≥ 600 ms), 25 ms debounce, polled every UI frame.
  Footer hint `tap ›  2× ‹  hold OK`.
* **EN (SW4)**: pulls EN to GND (C23 1 nF), i.e. reset. EN has a 12 kΩ
  pull-up (R1) and 0.1 µF (C9).
* **Sleep / wake**: `power_off` turns the backlight off, waits for GPIO0
  to be released, arms ext0 wake on GPIO0 low and enters deep sleep.
  `CAPS.wake_hint = "BOOT or EN"`.
* **Manual download mode**: hold BOOT, press and release EN, release
  BOOT (pulls GPIO0 and GPIO2 low at reset; boot-mode table in the
  module datasheet). Normally not needed (auto-program).

### 4.5 RGB LED

(Schematic, not used by firmware.) U6 "5050RGB": common anode to VDD33,
each cathode through 5.1 kΩ to a low-side LBSS138 MOSFET whose gate is
GPIO2 (Q6), GPIO0 (Q7) or GPIO4 (Q8) — a channel is lit while its GPIO is
**high**. Which colour sits on which GPIO is not labelled in the
schematic: **unknown** here. Inference from the schematic, not checked on
the bench: since GPIO0 idles high (BOOT key pull-up), the Q7 channel is
lit whenever the firmware runs and goes dark while BOOT is held.

### 4.6 microSD (and the LCD conflict)

* J4 TF_CARD, 4-bit: DATA0 = GPIO2, DATA1 = GPIO4, DATA2 = GPIO12,
  DATA3 = GPIO13, CLK = GPIO14, CMD = GPIO15 (0 Ω series resistors);
  10 kΩ pull-ups on DATA0, DATA1, DATA3, CLK, CMD; the DATA2 (GPIO12 =
  MTDI strapping) pull-up R157 is NC. Card detect: GPIO21 via R167 with a
  10 kΩ pull-up (R168). ESD diodes LESD8D3.3CAT5G. (Schematic.)
* **Conflict**: the card-detect switch closes with a card inserted and
  holds GPIO21 — the LCD's D/C line — low, so the LCD stays **white**.
  Piano Coach does not use the slot (`HAS=` is empty, songs come from
  flash). At start `lcd_init` configures GPIO21 as input with pull-up,
  waits 2 ticks and, if it reads low, logs
  `lcd: GPIO21 (D/C) is held low — a microSD card is in the slot; remove it, the LCD cannot work with it`.
  **Keep the slot empty.**
* The sdkconfig still enables FatFs LFN — only because the shared ESP-IDF
  checkout carries the exFAT patch, which needs it to compile.

### 4.7 Camera header

(Schematic, not used by firmware.) JP4, 2×9 female: 1 VDD33, 2 GND,
3 SIO_C = IO27, 4 SIO_D = IO26, 5 VSYNC = IO25, 6 HREF = IO23,
7 PCLK = IO22, 8 XCLK = IO21, 9 D7 = IO35, 10 D6 = IO34, 11 D5 = IO39,
12 D4 = IO36, 13 D3 = IO19, 14 D2 = IO18, 15 D1 = IO5, 16 D0 = IO4,
17 RESET = IO0, 18 PWDN (10 kΩ R60 to GND). Shares six pins with the LCD
and the backlight.

### 4.8 JTAG and the JP2 jumper block

(Schematic; JTAG not used by the project.) JP2 is a 2×14 header between
the FT2232 and the ESP32; the schematic says "When you use JTAG: Please
put Jumpers ON!". Pairs relevant here:

| JP2 pins | ESP32 side | FT2232 side |
|---|---|---|
| 3–4 | RXD (FT2232) | TXD0 (GPIO1) |
| 5–6 | TXD (FT2232) | RXD0 (GPIO3) |
| 15–16 | EN (R_nTRST) | nTRST |
| 17–18 | MTMS = GPIO14 | TMS |
| 19–20 | MTDO = GPIO15 | TDO |
| 21–22 | MTDI = GPIO12 | TDI |
| 23–24 | MTCK = GPIO13 | TCK |
| 25–26 | S_TDI | TDI |

Pins 1/2 are VDD33, 7/8 and 27/28 GND, 9–14 break out the module's flash
lines (SD0–SD3, CLK, CMD). The UART jumpers (3–4, 5–6) must be fitted for
the console and for flashing (they are on the bench unit — the console
works). A small circuit (Q3, D8, C32, R122 NC, R123) between TDI and
S_TDI exists; its purpose is not described in the schematic.

### 4.9 Radio (BLE)

* Native BLE controller, **BLE only** (`CONFIG_BTDM_CTRL_MODE_BLE_ONLY`),
  one connection, NimBLE central + observer only, no security, bonds
  persisted in NVS (max 2). `bt_controller_start` is a no-op (the
  controller starts inside `nimble_port_init`).
* **Host flow control OFF** (`CONFIG_BT_NIMBLE_HS_FLOW_CTRL=n`).
  ESP-IDF turns it on for the original ESP32 only; with it the controller
  stops handing the piano's packets to NimBLE once it has sent as many as
  the host has ACL buffers — and this board keeps only **12** ACL buffers,
  so it would stall after about 12 incoming packets. Symptom: key presses
  stop arriving, link drop 0x208, later connections get no ATT answers
  until a reset. Keep it off.
* Original-ESP32 behaviour compiled in by `cfg(esp32)` / `cfg(ble_relaxed)`
  (`build.rs` sets `ble_relaxed` for `MCU=esp32`):
  * The UI thread is pinned to **core 1** (`threads::spawn_core1`): the
    BT controller and NimBLE host run on core 0, and a busy UI there (plus
    the LCD's SPI interrupts, which follow the core that opened the panel)
    starved the link on the ESP32-2432S028.
  * BLE TX power is raised from the default +3 dBm to **+9 dBm** (default,
    scan and connection handle 0).
  * Link parameters: connect with, and after the piano's own
    renegotiation (10 ms interval / 500 ms supervision) ask again for, a
    **30–50 ms interval and 10 s supervision timeout** (`relax_link`, at
    most twice per connection), and gather outgoing MIDI into one write
    per **15 ms** (`WRITE_GAP`).
* The piano accepts one BLE central at a time: power off other boards and
  tablets when testing this one.

## 5. Memory map & partitions

`boards/wroverkit/partitions.csv` (4 MB flash, no OTA):

| Name | Type | SubType | Offset | Size | Notes |
|---|---|---|---|---|---|
| (bootloader) | — | — | 0x1000 | — | ESP32 default |
| (partition table) | — | — | 0x8000 | — | ESP-IDF default |
| nvs | data | nvs | 0x9000 | 24 KB (0x6000) | prefs (namespace `piano`, i32), best scores, BLE bonds |
| phy_init | data | phy | 0xF000 | 4 KB | |
| factory | app | factory | 0x10000 | 0x280000 (2.5 MB) | app was 2.27 MB at the port |
| songs | data | undefined | 0x290000 | 0x170000 (1472 KB) | song pack; ends at 0x400000 = 4 MB |

Song pack (`SONGPACK_ARGS`): `--max-bytes 1400K --max-notes 1200 --prefer
儿时回忆（41） --prefer 红色歌谣（52） --prefer 影视剧场（240）` — small
categories first, at most 1200 notes per song because RAM is internal
only (the same limit as the ESP32-C3 board). Result: 776 songs;
`glyphs.slint` holds 1344 distinct characters. Slint and the app are
built with `opt-level="s"` (`CARGO_CONFIG`) to fit 2.5 MB.

Notable `sdkconfig.defaults` settings, with the file's own reasons:

| Setting | Value | Why (quoted or from the commit) |
|---|---|---|
| `CONFIG_FREERTOS_HZ` | 1000 | "1 kHz tick: MIDI timing needs ms resolution." |
| `CONFIG_ESP_MAIN_TASK_STACK_SIZE` | 8192 | Port commit 16cc8a6: 4 KB overflowed during start-up on this Xtensa chip; the stack is freed when `app_main` returns. |
| `CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT` | 6144 | |
| `CONFIG_ESP_TASK_WDT_TIMEOUT_S` | 15 | |
| `CONFIG_SPIRAM` | n | "PSRAM stays off: ESP32 rev 1 needs a cache workaround in every piece of code that touches it, and the Rust code is not compiled with it." |
| `CONFIG_SPI_FLASH_ROM_IMPL`, `CONFIG_FREERTOS_PLACE_FUNCTIONS_INTO_FLASH`, `CONFIG_RINGBUF_PLACE_FUNCTIONS_INTO_FLASH` | y | move code out of IRAM to save internal RAM (no comment in the file) |
| `CONFIG_BTDM_CTRL_MODE_BLE_ONLY`, `CONFIG_BTDM_CTRL_BLE_MAX_CONN=1` | | "BLE only (no Classic BT memory), NimBLE central, one link, lean pools." |
| `CONFIG_BT_NIMBLE_ROLE_CENTRAL/OBSERVER=y`, `PERIPHERAL/BROADCASTER=n`, `MAX_CONNECTIONS=1`, `MAX_BONDS=2`, `MAX_CCCDS=4` | | same |
| `CONFIG_BT_NIMBLE_HOST_TASK_STACK_SIZE` | 5120 | |
| `CONFIG_BT_NIMBLE_ATT_PREFERRED_MTU` | 260 | piano packets ≤ 244 bytes |
| `CONFIG_BT_NIMBLE_NVS_PERSIST=y`, `SECURITY_ENABLE=n`, `LOG_LEVEL_WARNING=y` | | |
| `CONFIG_BT_NIMBLE_HS_FLOW_CTRL` | n | "with it this controller stops handing the piano's packets to NimBLE once it has sent as many as NimBLE has ACL buffers (24 by default): the host's buffer credits evidently never come back." (see 4.9) |
| `CONFIG_BT_NIMBLE_GATT_MAX_PROCS` | 2 | lean pools |
| `CONFIG_BT_NIMBLE_MSYS_1_BLOCK_COUNT` / `CONFIG_BT_NIMBLE_ACL_BUF_COUNT` | 12 / 12 | trimmed pools for internal RAM |
| `CONFIG_FATFS_LFN_HEAP`, `MAX_LFN=255`, `USE_LABEL` | | "No microSD here, but the shared ESP-IDF checkout carries the exFAT patch (tools/patch-idf-exfat.sh), which needs LFN on to compile FatFs at all." |
| `CONFIG_ESPTOOLPY_FLASHMODE_DIO`, `FLASHSIZE_4MB`, `PARTITION_TABLE_CUSTOM` | | |

RAM: about 145 KB heap free at run time (port log); the mini UI logs
`ui: heap free … KB (min …, largest block …)` — watch it, there is no
PSRAM. Pieces in `threads.rs` that put stacks in PSRAM are compiled out
here (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY` is not set).

## 6. Build, flash, console

From `firmware/piano-coach/`:

```bash
# firmware (first build compiles ESP-IDF: 10+ min — run in the background)
tools/board.sh wroverkit build --release

# song pack (only when the library or SONGPACK_ARGS change; rebuild the
# firmware afterwards if boards/wroverkit/glyphs.slint changed)
tools/songs.sh wroverkit build
tools/songs.sh wroverkit flash --port /dev/ttyUSB1 --baud 921600   # writes at 0x290000

# flash + monitor (blocks): cargo runner flash.sh = espflash flash --monitor
# with this build's bootloader.bin and partition-table.bin
tools/board.sh wroverkit run --release -- --port /dev/ttyUSB1

# or directly
T=target/wroverkit/xtensa-esp32-espidf/release
espflash flash --chip esp32 --port /dev/ttyUSB1 --baud 921600 \
  --bootloader $T/bootloader.bin --partition-table $T/partition-table.bin $T/piano-coach
```

(esp-idf-sys may place `bootloader.bin` / `partition-table.bin` in
`$T/build/`; `flash.sh` checks there first.)

* `tools/board.sh` sources `board.env`: `FEATURE=board-wroverkit`,
  `MCU=esp32`, `ESP_IDF_VERSION=v5.5.3`, `TARGET=xtensa-esp32-espidf`,
  `TOOLCHAIN=esp198` (runs `cargo +esp198` and sets `LIBCLANG_PATH` from
  `~/.rustup/toolchains/esp198/xtensa-esp32-elf-clang/*/esp-clang/lib`),
  `FLASHER=espflash`, `UI=mini`, `HAS=` (none). ESP-IDF tools are shared
  from `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`;
  target dir `target/wroverkit`, generated sdkconfig `sdkconfig.wroverkit`.
* No extra C components: `esp-idf-components.toml` only generates the
  bindings header (`src/bindings.h`, branch `CONFIG_IDF_TARGET_ESP32`,
  which includes `esp_lcd_panel_vendor.h`).
* Flashing the app keeps NVS and the song pack. Writing an NVS image at
  0x9000 (see AGENTS.md) replaces all prefs and BLE bonds.
* **Port**: console/flash = FT2232 interface 1 (`/dev/ttyUSB1` usually);
  never flash through interface 0 (JTAG). See 4.2 for identification.
* **Reset / download**: espflash resets and enters download mode through
  DTR/RTS (4.2). A serial monitor that asserts DTR/RTS resets the board.
  If auto-program fails, hold BOOT and tap EN.
* Console: UART0, 115200 baud (ESP-IDF default; not overridden in
  `sdkconfig.defaults`). Useful lines: `board: ESP-WROVER-KIT v4.1`,
  `lcd: ID … → ILI9341`, `display up: 320x240 LCD panel`, the
  `ble-midi:` link / traffic lines.

## 7. Piano Coach support

* **UI class**: `mini` at 1.25× (wheel menu library, song menu, play
  screen; Settings incl. brightness, Light all keys, All LEDs off, Sleep).
  `CAPS`: backlight yes, power latch no, USB MIDI no, BLE MIDI yes,
  buttons yes (one key), touch no.
* **Songs**: 776 from the flash pack (≤ 1200 notes each).
* **Boot**: Follow mode with a random 儿时回忆 song that starts when the
  piano connects (common to all boards).

Verified on hardware:

| Date | What | Result |
|---|---|---|
| 2026-10-16 (port, 16cc8a6) | boot, ILI9341 detected by its zero ID, display, 776 songs, BLE, BOOT navigation, Learn | ✔ — app 2.27 MB of 2.5 MB, 145 KB heap free |
| 2026-10-16 (993d9a7) | two-way BLE with flow control off: Follow with both hands, nine songs started from the piano's control keys | ✔ — 10 minutes, 1340 notifications in, 6328 writes out, no drop, no busy retry, RSSI −74…−83 dBm |

**Not verified on this board** (say so before claiming them):

* The **ST7789V branch** (its init table, RGB order, mirror X) — the
  bench unit has an ILI9341; the ST7789V path has never run here.
* The full piano-key remote (song keys were used during the BLE test, with
  the old folder stepping; E1/F1 by category, tempo, modes, hands not
  separately checked), shuffle-play's automatic next song.
* Sleep (deep sleep + ext0 wake on BOOT) and backlight dimming — not
  separately recorded.

Known limits: no bookmarks (C8 does nothing on small boards), no search,
A–B loop or sheet view on the mini UI; no speaker (`HAS` has no
`speaker`), no USB host, no microSD.

## 8. Quirks & lessons learned

* **microSD blanks the LCD**: the card-detect switch pulls GPIO21 (LCD
  D/C) low. Keep the slot empty; the boot log warns.
* **8 KB main-task stack**: 4 KB overflowed during start-up (symptom:
  BREAK at a heap address, 0xa5 stack fill in EXCVADDR).
* **PSRAM unusable** on this chip revision with Rust code; plan for
  ~145 KB heap and the 1200-note song limit.
* **BLE host flow control must stay off** on the original ESP32 (with only
  12 ACL buffers here it would stall even sooner than on the CYD).
* **UI on core 1**, TX power +9 dBm, relaxed link parameters — all
  automatic for `MCU=esp32`; keep them when touching `threads.rs` /
  `ble_midi.rs`.
* **SPI clock 26 MHz max**: the LCD clock (GPIO19) is not an IOMUX SPI
  clock pin.
* **BOOT also pulls GPIO2 low**; GPIO2 and GPIO0 are strapping pins and
  also drive RGB LED channels. GPIO12 must be low at reset (VDD_SDIO
  strapping) — the board leaves its pull-ups unfitted.
* **GPIO32/33** carry the 32.768 kHz crystal, not the header, as shipped.
* **GPIO16/17** belong to the module's PSRAM even though JP1 breaks them
  out.
* **Port numbering**: two ttys per board; the console is interface 01.
  Another FTDI device on the PC shifts the numbers.
* espflash on this board: use `--chip esp32` when calling it directly.

## 9. Sources

In this repo:

* `firmware/piano-coach/boards/wroverkit/board.env`, `sdkconfig.defaults`,
  `partitions.csv`, `esp-idf-components.toml`, `glyphs.slint`
* `firmware/piano-coach/src/board/wroverkit/mod.rs`,
  `src/board/mod.rs`, `src/board/contract.rs`, `src/board/spi_lcd.rs`,
  `src/board/key.rs`, `src/threads.rs`, `src/main.rs` (`spawn_ui`),
  `src/ble_midi.rs`, `src/bindings.h`, `build.rs`, `flash.sh`,
  `.cargo/config.toml`, `tools/board.sh`, `tools/songs.sh`
* `AGENTS.md`, `doc/piano_coach_firmware.md` (small boards, build, "BLE on
  the original ESP32", verified-on-hardware paragraphs),
  `firmware/piano-coach/boards/README.md`
* Commits 16cc8a6 (port), a7c4765 (flow control off), 993d9a7 (WROVER
  two-way BLE test)
* [vendor/wroverkit/ESP-WROVER-KIT_V4_1.pdf](vendor/wroverkit/ESP-WROVER-KIT_V4_1.pdf)
  — the vendor schematic, a single A-size sheet (Rev 1.0, 2018-08-29);
  there is no prose user guide in it.

Outside the repo (read, not copied — chip/module datasheets):

* `/home/nws/w/esp32/doc/ESP-WROVER-KIT-v4.1/esp32-wrover-e_esp32-wrover-ie_datasheet_en.pdf`
  (v2.4: pin layout and functions, strapping pins, boot modes)
* `/home/nws/w/esp32/doc/ESP-WROVER-KIT-v4.1/esp32_datasheet_en.pdf`
  (chip revisions per part number, PSRAM pin mapping GPIO16/17)

Discrepancies and open points found while writing:

* Module: the task description called it ESP32-WROVER-E, but the chip is
  revision 1.0, which the WROVER-E (ESP32-D0WD-V3, rev v3.x) cannot be.
  The exact module variant and PSRAM size are unknown.
* FT2232 VID:PID not observed (board not attached when this was written).
* The schematic's 32 kHz note names R36/R37 (crystal) and R38/R39 (GPIO),
  but the drawn parts are R11/R23 (fitted) and R12/R24 (NC); R36/R37 are
  the 0 Ω resistors on IO12/IO13 elsewhere on the sheet. Follow the
  drawing.
* `AGENTS.md` describes the relaxed link as "30–50 ms / 4 s"; the code
  (`ble_midi.rs` `SUPERVISION_10MS = 1000`) asks for 10 s. Code wins.
* RGB colour per GPIO is not labelled in the schematic.
