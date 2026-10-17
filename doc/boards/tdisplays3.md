# LILYGO T-Display-S3 — hardware reference

Board dir `tdisplays3`. Everything a developer needs to work on Piano Coach
for this board without the vendor's repository. Facts the firmware
exercises come from the code in `firmware/piano-coach/` and are known to
work on the bench unit; facts marked "(schematic, not used by firmware)" or
"(vendor docs)" come only from LILYGO's material and were not checked on
the bench. The schematic is copied to
[vendor/tdisplays3/T_Display_S3.pdf](vendor/tdisplays3/T_Display_S3.pdf).

## 1. Summary

The T-Display-S3 is a small bar-shaped board: an ESP32-S3R8 (8 MB octal
PSRAM in the package) with a 16 MB QSPI flash, a 1.9″ 170×320 IPS LCD
driven by an ST7789 over an 8-bit Intel-8080 parallel bus, two push
buttons (BOOT and a user key), a reset button, a Li-ion charger with a
battery connector, a USB-C port wired to the S3's native USB, two 12-pin
headers and a 4-pin UART connector. A "Touch" variant adds a capacitive
touch panel on I2C. Piano Coach runs the **mini UI** on it in landscape
(320×170), keeps the whole song library (2449 songs) in flash and talks to
the piano over **BLE only** (no USB host, microSD or speaker).

| Item | Value | Source |
|---|---|---|
| Vendor / product | LILYGO (Xinyuan) T-Display-S3; T-Display-S3-Touch is the touch variant | vendor README |
| Board revision | Schematic dated 2024-04-29 (no revision field filled in); the vendor pin-map photo shows the silkscreen "T_Display_S3 22-5-18 V1.0". The bench unit's PCB revision was not recorded | schematic, `image/T-DISPLAY-S3.jpg` |
| Bench unit | ESP32-S3R8 rev 0.2, **no touch** | `doc/piano_coach_firmware.md` |
| Board dir | `firmware/piano-coach/boards/tdisplays3/`, `src/board/tdisplays3/mod.rs` | code |
| Cargo feature | `board-tdisplays3` | `board.env`, `Cargo.toml` |
| MCU | ESP32-S3R8 (U2), dual-core Xtensa LX7, 240 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240`) | schematic, `sdkconfig.defaults` |
| Rust target / toolchain | `xtensa-esp32s3-espidf`, esp-rs toolchain `esp198` (`espup install --name esp198 --toolchain-version 1.98.1.0`) | `board.env`, AGENTS.md |
| ESP-IDF | v5.5.3 (`ESP_IDF_VERSION=v5.5.3`, shared install under `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`) | `board.env`, `tools/board.sh` |
| Flash | 16 MB, external W25Q128JV (U3) on the S3's SPI0/1 pins, QIO, 80 MHz (generated sdkconfig) | schematic, `sdkconfig.defaults` |
| PSRAM | 8 MB octal, in the ESP32-S3R8 package, 80 MHz (`CONFIG_SPIRAM_MODE_OCT`, `CONFIG_SPIRAM_SPEED_80M`) | vendor README, `sdkconfig.defaults` |
| Crystal | 40 MHz (U1) | schematic |
| USB | USB-C → ESP32-S3 native USB (GPIO19 D−, GPIO20 D+). Piano Coach uses the **USB Serial/JTAG** controller as console: Linux shows `/dev/ttyACM*`. The exact VID:PID was not recorded for this board (the Espressif USB Serial/JTAG normally enumerates as 303a:1001, as the Tab5 does) | schematic, `sdkconfig.defaults` |
| Display | 1.9″ IPS, 170×320 native, ST7789, 8-bit i80, used as 320×170 landscape | code, vendor README |
| Touch (variant only) | CST816 at I2C 0x15 (code); some early units used a CST328 at 0x1A (vendor examples) | code, vendor examples |
| Radio | Wi-Fi + BLE 5 of the S3; on-board antenna (an external-antenna option by moving a 0 Ω resistor) | vendor README, schematic |

## 2. Feature overview

| Part (ref) | Function | Bus / interface | Address / pins | Used by Piano Coach |
|---|---|---|---|---|
| ESP32-S3R8 (U2) | MCU + 8 MB OPI PSRAM | — | — | yes |
| W25Q128JV (U3) | 16 MB flash | SPI0/1 (SPICS0, SPICLK, SPID, SPIQ, SPIWP, SPIHD) | — | yes (app + song pack) |
| 40 MHz crystal (U1) | main clock | — | XTAL_P / XTAL_N | yes |
| ST7789 LCD, 1.9″ 170×320 (via 30-pin FPC CN1, KH-FG0.5-H2.0-30PIN) | display | 8-bit i80 | D0–D7 = GPIO39, 40, 41, 42, 45, 46, 47, 48; WR 8, RD 9, DC 7, CS 6, RST 5 | yes |
| AW9364DNR (U4) | LCD backlight LED driver (4 LED cathode channels LEDK1–4 to CN1) | EN pin | EN = GPIO38 (`ESP_LCD_BLK`); VIN = V3V | yes (LEDC PWM on GPIO38) |
| CST816 touch controller (Touch variant, on the panel) | capacitive touch | I2C | 0x15; SDA 18, SCL 17, RST 21, INT 16 | yes, probed at boot; INT not used |
| CST328 touch controller (early Touch units, vendor docs) | capacitive touch | I2C | 0x1A | no (not probed) |
| BOOT button (S2) | strapping / user key | GPIO | GPIO0, 10 kΩ pull-up (R1), active low | yes |
| KEY button (S3) | user key | GPIO | GPIO14, 100 kΩ pull-up (R3) + 100 nF (C14) to GND, active low | yes |
| RST button (SW) | reset | CHIP_PU | 10 kΩ pull-up (RW), 1 µF (CU) | — (hardware) |
| Battery divider R2/R4 (100 kΩ 1 % each) + C13 10 nF | battery voltage sense, ½ of BAT | ADC | GPIO4 (`BAT_ADC`) | no (schematic, not used by firmware) |
| TP4065 (U6) | Li-ion charger, R13 = 2 kΩ, schematic note "IBAT (mA) = 580" | — | CHRG → red LED1 | no |
| AP2112K-3.3 / RT9013 (U7) | 3.3 V LDO (VDD3V3) from the +5V node | — | — | — (hardware) |
| SI2301 (Q3) + PMBT3904 (Q1) | switches VDD3V3 → V3V (backlight driver, LCD pin 20, green LED) when VBUS is present **or** GPIO15 (`PWR_EN`) is high | GPIO | GPIO15 | yes (driven high at boot) |
| SI2301 (Q2), SI2307 (Q4), 1N5819 (D3) | battery / VBUS power path into the +5V node | — | — | — (hardware) |
| Red LED (LED1) | charge indicator (TP4065 CHRG) | — | — | no |
| Green LED (LED8) | on the V3V rail via 2 kΩ: lit while V3V is on | — | — | no (follows GPIO15 on battery) |
| USB-C receptacle (U5) | power + native USB | USB 1.1 FS | GPIO19 / GPIO20 | yes (console, flashing) |
| Battery connector (P3) | 2-pin, JST 1.25 mm (vendor README), through fuse R10 ("5V/2A") | — | VBAT | no |
| Header P2 (12-pin) | GPIO1, 2, 3, 10, 11, 12, 13, +5V, GND, 3V3 | — | see §3 | no |
| Header P1 (12-pin) | UART0, I2C, touch RST/INT, GND, 3V3 | — | see §3 | no |
| CNC (4-pin WAFER-100W-4A, 1.0 mm) | GND, 3V3, U0TXD (GPIO43), U0RXD (GPIO44) — a **UART** connector, not Qwiic/STEMMA I2C | UART0 | GPIO43/44 | no |

There is no microSD slot, no audio codec or speaker, no USB-A host port
and no addressable LED. (The vendor's SD pins 11/12/13 belong to an
add-on "T-Display-TF Shield", see §4.9.)

## 3. Pin map

"Code" = the firmware drives or reads the pin; "schematic" / "vendor"
= only documented by LILYGO.

| GPIO | Function (net) | Dir / active level | Peripheral | Used by firmware | Source |
|---|---|---|---|---|---|
| 0 | BOOT button (S2), strapping pin | in, active low, 10 kΩ pull-up (R1) | GPIO; ext0 deep-sleep wake | yes: "◀ / back" key, wake source | code, schematic |
| 1 | header P2 pin 11 | — | — | no | schematic |
| 2 | header P2 pin 10 | — | — | no | schematic |
| 3 | header P2 pin 9 | — | — | no | schematic |
| 4 | `BAT_ADC`: ½ × battery voltage | analog in | ADC1 | no | schematic, vendor `pin_config.h` |
| 5 | `ESP_LCD_RES` LCD reset | out, active low | esp_lcd (panel `reset_gpio_num`) | yes | code, schematic |
| 6 | `ESP_LCD_CS` | out, active low | i80 CS | yes | code, schematic |
| 7 | `ESP_LCD_D/C` | out, high = data | i80 DC | yes | code, schematic |
| 8 | `ESP_LCD_WR` | out | i80 WR (pixel clock) | yes | code, schematic |
| 9 | `ESP_LCD_RD` | out, held high | plain GPIO | yes: driven high in `early_init` (the panel is never read) | code, schematic |
| 10 | header P2 pin 8 | — | — | no | schematic |
| 11 | header P2 pin 7 (vendor: SD CLK on the TF shield) | — | — | no | schematic, vendor README |
| 12 | header P2 pin 6 (vendor: SD D0 on the TF shield) | — | — | no | schematic, vendor README |
| 13 | header P2 pin 5 (vendor: SD CMD on the TF shield) | — | — | no | schematic, vendor README |
| 14 | KEY button (S3) | in, active low, 100 kΩ pull-up (R3) + 100 nF | GPIO | yes: "▶ / OK" key | code, schematic |
| 15 | `PWR_EN`: switches the V3V rail (backlight, LCD pin 20, green LED) on battery | out, active high | GPIO | yes: driven high first thing in `early_init`, low in `power_off` | code, schematic |
| 16 | `ESP_TP_INT` touch interrupt; header P1 pin 5 | in, 100 kΩ pull-up (RINT) | — | no (touch is polled) | schematic, vendor `pin_config.h` |
| 17 | `II2C_SCL`; header P1 pin 7 | open drain, 10 kΩ pull-up (RIIC) | I2C0 | yes (touch, if fitted) | code, schematic |
| 18 | `II2C_SDA`; header P1 pin 8 | open drain, 10 kΩ pull-up (RIID) | I2C0 | yes (touch, if fitted) | code, schematic |
| 19 | `ESP_USB_DM` | — | USB Serial/JTAG | yes (console) | schematic, `sdkconfig.defaults` |
| 20 | `ESP_USB_DP` | — | USB Serial/JTAG | yes (console) | schematic, `sdkconfig.defaults` |
| 21 | `ESP_TP_RESET` touch reset; header P1 pin 6 | out, active low | GPIO | yes: pulsed low 10 ticks, then high, before the I2C probe | code, schematic |
| 33–37 | not connected on the board (pins 38–42 of U2 marked "X") | — | — | — | schematic |
| 38 | `ESP_LCD_BLK` → AW9364 EN | out, active high | LEDC timer 0 / channel 0, 20 kHz, 10 bit | yes | code, schematic |
| 39 | `ESP_LCD_D0` | out | i80 data | yes | code, schematic |
| 40 | `ESP_LCD_D1` | out | i80 data | yes | code, schematic |
| 41 | `ESP_LCD_D2` | out | i80 data | yes | code, schematic |
| 42 | `ESP_LCD_D3` | out | i80 data | yes | code, schematic |
| 43 | `U0TXD`; header P1 pin 10; CNC pin 3 | out | UART0 TX | no (console is on USB) | schematic |
| 44 | `U0RXD`; header P1 pin 9; CNC pin 4 | in | UART0 RX | no | schematic |
| 45 | `ESP_LCD_D4` | out | i80 data | yes | code, schematic |
| 46 | `ESP_LCD_D5` | out | i80 data | yes | code, schematic |
| 47 | `ESP_LCD_D6` (SPICLK_P pin) | out | i80 data | yes | code, schematic |
| 48 | `ESP_LCD_D7` (SPICLK_N pin) | out | i80 data | yes | code, schematic |

Note that the LCD data bus uses GPIO39–42 (the S3's pad-JTAG pins MTCK,
MTDO, MTDI, MTMS per the schematic's pin names): pad JTAG is not available
while the display runs; debugging goes through the USB Serial/JTAG.

Headers (schematic; pin 1 is the +5V / 3V3 end, as in the vendor
pin-map photo):

| Pin | P2 | P1 |
|---|---|---|
| 12 | 3V3 (VDD3V3) | GND |
| 11 | GPIO1 | GND |
| 10 | GPIO2 | GPIO43 (U0TXD) |
| 9 | GPIO3 | GPIO44 (U0RXD) |
| 8 | GPIO10 | GPIO18 (I2C SDA) |
| 7 | GPIO11 | GPIO17 (I2C SCL) |
| 6 | GPIO12 | GPIO21 (touch RST) |
| 5 | GPIO13 | GPIO16 (touch INT) |
| 4 | NC | NC |
| 3 | NC | GND |
| 2 | GND | GND |
| 1 | +5V | 3V3 (VDD3V3) |

## 4. Subsystems

### 4.1 Power and battery

From the schematic (none of this is exercised by the firmware except
GPIO15):

* USB VBUS reaches the +5V node through a 1N5819 Schottky (D3); a battery
  (VBAT, 3.7–4.2 V, connector P3 via fuse R10) reaches it through
  P-MOSFETs (Q4 SI2307, Q2 SI2301). The AP2112K-3.3 LDO (U7, alternative
  RT9013) makes VDD3V3 for the ESP32-S3, flash, LCD logic supply (CN1 pin
  2) and the I2C/touch pull-ups.
* **V3V** is a switched copy of VDD3V3 (Q3 SI2301, driven by Q1
  PMBT3904). Q1's base is a diode-OR (1N4148 D1/D2) of VBUS and `PWR_EN` =
  GPIO15, with a 100 kΩ pull-down (R7). So on USB power V3V is always on;
  **on battery V3V is on only while GPIO15 is high**. V3V feeds the
  AW9364 backlight driver (VIN), LCD connector pin 20 and the green LED.
  The schematic note reads "Battery power must set IO15 to HIGH"; the
  vendor README says the LCD and other peripherals do not work otherwise.
* The firmware drives GPIO15 high in `early_init()` (before anything
  touches the LCD) and low in `power_off()` before deep sleep. Whether
  GPIO15 stays low during deep sleep (no `gpio_hold_en` is used) was not
  measured.
* Charger: TP4065 (U6), VBUS in, programming resistor R13 = 2 kΩ with the
  schematic note "IBAT (mA) = 580"; the vendor README says the default
  charge current is 500 mA (the two disagree; neither matters to the
  firmware). Red LED1 on CHRG: on while charging, off when full, flashing
  or dim with no battery (vendor README).
* Battery voltage: BAT → 100 kΩ / 100 kΩ (1 %) divider → GPIO4, 10 nF to
  ground. The vendor example reads it with ADC1, 11 dB attenuation, and
  multiplies by 2; above 4300 mV it reports "no battery". The vendor README
  says the reading is only valid while USB-C is unplugged.
  (schematic, not used by firmware)
* Electrical limits (vendor README): USB-C 5 V in; 3V header pin load
  ≤ 100 mA; the 5V header pin is VBUS from USB-C.
* Piano Coach has no power latch: `CAPS.power_latch = false`; "Sleep" is
  ESP32 deep sleep (§7).

### 4.2 USB

The USB-C receptacle's D+/D− go straight to the S3 (GPIO20 / GPIO19); there
is no USB-UART bridge. Piano Coach sets `CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG=y`,
so the port is the S3's USB Serial/JTAG controller (log console, flashing
via espflash's USB-JTAG reset, JTAG debugging). The board is a USB device
only; the firmware has `HAS=` (no `usb`), so no USB host and no USB-MIDI to
the piano. The UART0 pins (GPIO43/44) are on header P1 and the 4-pin CNC
connector but unused.

### 4.3 Display (ST7789, 8-bit i80)

Code: `src/board/tdisplays3/mod.rs` (`lcd_init`) and
`src/board/spi_lcd.rs` (`open_io_i80`, `SpiLcd`).

| Setting | Firmware value | Vendor examples |
|---|---|---|
| Bus | `esp_lcd_new_i80_bus`, 8-bit, `LCD_CLK_SRC_DEFAULT`, D0–D7 = 39, 40, 41, 42, 45, 46, 47, 48, DC 7, WR 8 | same pins; lv_demos uses `LCD_CLK_SRC_PLL160M` |
| Panel IO | `esp_lcd_new_panel_io_i80`, CS 6, 8-bit cmd/param, DC high for data, queue depth 4 | same, queue depth 10–20 |
| Pixel clock | **16 MHz** (verified) | factory: 16 MHz ("too low or too high pixel clock may cause screen mosaic"); lv_demos: 6.528 MHz; LilyGo-Display-IDF: 10 MHz |
| Max transfer | 320 × 10 × 2 = 6400 bytes (one 10-line band) | — |
| RD | GPIO9 held high as a plain output | same |
| Reset | GPIO5 via `reset_gpio_num` (`esp_lcd_panel_reset`) | same |
| Driver | ESP-IDF `esp_lcd_new_panel_st7789`, RGB element order, 16 bpp | same |
| Inversion | `esp_lcd_panel_invert_color(true)` (IPS) | same |
| Orientation | `swap_xy(true)`, `mirror(false, true)` | same; LilyGo-Display-IDF comments this as "the screen faces you, and the USB is on the left" (`mirror(true, false)` = USB on the right) |
| Offset | `set_gap(0, 35)` | same ("the gap is LCD panel specific") |
| Resolution used | `LCD_WIDTH` 320 × `LCD_HEIGHT` 170, `UI_SCALE` 1.0, `ROUND` false | 320 × 170 |
| Mirror setting | `LCD_MIRROR = None` ("the panel's orientation is the same on every unit") | — |

**Init sequence.** After `esp_lcd_panel_init` (which in ESP-IDF v5.5.3
sends SLPOUT + 100 ms, MADCTL and COLMOD 0x55) and the orientation calls,
the firmware sends the vendor's "LCD_MODULE_CMD_1" sequence, needed by
newer panels:

| Cmd | Parameters |
|---|---|
| B2 (porch) | 0B 0B 00 33 33 |
| B7 (gate) | 75 |
| BB (VCOM) | 28 |
| C0 | 2C |
| C2 | 01 |
| C3 | 1F |
| C6 (frame rate) | 13 |
| D0 | A7 |
| D0 (power) | A4 A1 |
| D6 | A1 |
| E0 (gamma +) | F0 05 0A 06 06 03 2B 32 43 36 11 10 2B 32 |
| E1 (gamma −) | F0 08 0C 0B 09 24 2B 22 43 38 15 16 2F 37 |

The vendor table starts with `11` (SLPOUT, 120 ms delay) and `3A 05`
(COLMOD); the firmware omits both because the ESP-IDF driver already sends
SLPOUT and COLMOD. Then `SpiLcd::finish` clears the panel to the
background in 4-line bands and turns the display on.

**Drawing.** No framebuffer: Slint renders 10-line bands into an internal
DMA buffer (`alloc_band`), `SpiLcd::draw` byte-swaps them to big-endian
RGB565 and waits for the transfer-done interrupt (binary semaphore, 100 ms
timeout).

**Backlight.** GPIO38 drives the EN pin of the AW9364DNR LED driver
(VIN = V3V). The firmware runs LEDC low-speed timer 0 / channel 0 at
20 kHz, 10-bit, not inverted (`backlight_init(38, false)`), and maps 0–100 %
to duty 0 or ≥ 40/1023 (`backlight_set`). The vendor examples only switch
GPIO38 high/low. The AW9364 datasheet is not in the vendor folder, and how
smoothly brightness steps dim on this board was not recorded — unknown.

**FPC CN1** (30-pin, read from the schematic): 1–4 GND/VDD3V3 (pin 2 =
VDD3V3), 5 RES, 6 CS, 7 D/C, 8 WR, 9 RD, 10 GND, 11–18 D0–D7, 20 V3V,
21–24 LEDK1–4, 25 GND, 26 TP_RESET, 27 SCL, 28 SDA, 29 TP_INT, 30 GND.
(schematic, not used by firmware beyond the signals above)

### 4.4 Touch (T-Display-S3-Touch only)

Code: `src/board/cst816.rs`, `touch_point()` in the board module.

* At `lcd_init` the firmware pulses GPIO21 low (10 ticks) then high (60
  ticks), creates I2C master bus 0 on SDA 18 / SCL 17 (internal pull-ups
  enabled, glitch filter 7) and a device at **0x15**, 400 kHz, and writes
  `FE FF` (register 0xFE DisAutoSleep ≠ 0: never enter auto low-power,
  which would stop it answering a polled read). If that write fails the
  log says "touch: none (T-Display-S3 without touch) — keys only" and the
  board runs with keys only — this is what the bench unit (no touch) does.
* Reading: register 0x02 onward, 5 bytes: FingerNum (low nibble), XposH
  (low nibble), XposL, YposH (low nibble), YposL (CST816S register
  document). Polled from the UI loop; INT (GPIO16) is not used.
* Mapping in the firmware: the controller reports the portrait panel
  (x 0–169 across, y 0–319 down); landscape `(lx, ly) = (y, 169 − x)`,
  points outside 320×170 dropped. **Untested** — the bench unit has no
  touch.
* **Discrepancy:** LilyGo-Display-IDF (`main/touch_driver.cpp`, same
  display orientation: swap XY, `mirror(false, true)`, gap 0/35) configures
  SensorLib with `setMaxCoordinates(320, 170)`, `setSwapXY(true)`,
  `setMirrorXY(true, false)`, i.e. `(lx, ly) = (320 − y, x)` — rotated 180°
  from the firmware's mapping. Neither has been tried on a touch unit
  with Piano Coach. If taps land diagonally opposite on a Touch unit,
  switch `touch_point` to the vendor mapping.
* The vendor's Arduino CapacitiveTouch example registers a virtual "home"
  button at raw (85, 360), i.e. beyond the 320-row panel; the firmware's
  bounds check drops such points.
* **Second controller:** the vendor Arduino examples try a CST328 at
  **0x1A** first ("Early use of CST328"), then a CST816 at 0x15. The
  firmware only probes 0x15, so a CST328 unit would run as "no touch"
  (keys still work). The CST328 datasheet is in the vendor folder, not in
  this repo.

### 4.5 Buttons

| Button | GPIO | Hardware | Firmware use |
|---|---|---|---|
| BOOT (S2) | 0 | 10 kΩ pull-up, to GND when pressed; also the ROM download-mode strap | `Key::new(0)`: tap = previous row, double tap = 5 rows back, hold ≥ 0.6 s = back; on the play screen tap is ignored, hold = back; deep-sleep wake (ext0, low) |
| KEY (S3) | 14 | 100 kΩ pull-up, 100 nF to GND (RC debounce) | `Key::new(14)`: tap = next row, double tap = 5 rows, hold = OK; on the play screen tap = pause / resume |
| RST (SW) | CHIP_PU | 10 kΩ / 1 µF | hardware reset; also wakes from sleep |

`src/board/key.rs` configures each key as an input with the internal
pull-up enabled, debounces 25 ms, reports a tap after a 280 ms
double-tap window, a hold at 600 ms; polled every UI frame. Footer hint
(`CAPS.key_hint`): "KEY ›  BOOT ‹  hold: OK / back".

Physical layout (vendor pin-map photo): with the screen facing you and
USB-C at the bottom, BOOT is left of the USB-C port and IO14 right of it.
In Piano Coach's landscape orientation (USB on the left, per the
LilyGo-Display-IDF comment for this orientation) BOOT should therefore be
at the top and KEY at the bottom of the left edge — derived from the
photo, not checked on the unit.

### 4.6 LEDs

Only the red charge LED (TP4065 CHRG) and the green V3V LED (lit while V3V
is on: always on USB power, follows GPIO15 on battery). Neither is
software-controllable apart from GPIO15. No RGB / WS2812 LED.

### 4.7 Storage (song pack)

No microSD. The `songs` data partition (11.9 MB, §5) holds a Piano Coach
song pack built by `tools/songs.sh tdisplays3 build` (`piano-core`
`songpack`, `SONGPACK_ARGS="--max-bytes 11.9M"`): the whole library, 2449
songs; the bench build is 9,347,030 bytes. `src/songs.rs` finds the
partition by name, checks the magic and size in the header and
memory-maps the occupied part (`esp_partition_mmap`, DATA). The same
build writes `boards/tdisplays3/glyphs.slint` (1890 distinct characters
for the mini UI's SDF font); rebuild the firmware when it changes.

### 4.8 Radio

ESP32-S3 Wi-Fi / BLE 5 with an on-board antenna. The schematic shows the
RF path with a 0 Ω option (RX1 / RX2(NC)); the vendor README says moving
that resistor selects an external antenna connector. Piano Coach uses
**BLE only** (NimBLE central + observer, one connection, preferred MTU
260, bonds persisted in NVS), Wi-Fi is not started. The board is not
`ble_relaxed` (`build.rs`: only headless boards and the original ESP32),
so it accepts the piano's own 10 ms interval / 500 ms supervision and
writes without a gap; NimBLE host flow control stays off (ESP-IDF default
on the S3; confirmed in the generated sdkconfig).

### 4.9 Connectors and expansion

* **P1 / P2** 12-pin headers (§3).
* **CNC** 4-pin 1.0 mm (WAFER-100W-4A): 1 GND, 2 3V3, 3 GPIO43 (U0TXD),
  4 GPIO44 (U0RXD). The vendor photo labels it "G 3V 43 44". It is UART0,
  not a Qwiic/STEMMA QT I2C port.
* **P3** battery, 2-pin JST 1.25 mm (vendor README).
* Vendor add-ons, not used: "T-Display-TF Shield" (SD on GPIO13 CMD,
  GPIO11 CLK, GPIO12 D0 — vendor README: "only available on boards with
  the SD Shield add-on"), and the T-Display-S3-MIDI shield (its own
  schematic, `SCH_T-Display-S3-MIDI_V1.1.pdf`, not copied here).

## 5. Memory map and partitions

`boards/tdisplays3/partitions.csv` (16 MB):

| Name | Type | SubType | Offset | Size | Notes |
|---|---|---|---|---|---|
| (bootloader) | — | — | 0x0 | — | ESP32-S3 bootloader at 0x0 |
| (partition table) | — | — | 0x8000 | — | |
| nvs | data | nvs | 0x9000 | 24 KB | prefs (namespace `piano`, i32), scores, BLE bonds |
| phy_init | data | phy | 0xF000 | 4 KB | |
| factory | app | factory | 0x10000 | 4 MB | app ~2.9 MB (doc, 2026-10-16) |
| songs | data | undefined | 0x410000 | 0xBF0000 (≈ 11.9 MB) | song pack, ends exactly at 16 MB |

No OTA partitions. Flashing the app does not touch `nvs` or `songs`; the
pack is written separately.

Notable `boards/tdisplays3/sdkconfig.defaults` settings and why:

| Setting | Why |
|---|---|
| `ESPTOOLPY_FLASHMODE_QIO`, `FLASHSIZE_16MB`, custom `partitions.csv` | 16 MB QSPI flash with the songs partition |
| `ESP_DEFAULT_CPU_FREQ_MHZ_240` | maximum CPU clock (no reason given in the file) |
| `ESP_CONSOLE_USB_SERIAL_JTAG=y` | the USB-C port is the native USB; no UART bridge |
| `FREERTOS_HZ=1000` | MIDI timing needs ms resolution |
| `ESP_MAIN_TASK_STACK_SIZE=6144`, `PTHREAD_TASK_STACK_SIZE_DEFAULT=8192`, `ESP_TASK_WDT_TIMEOUT_S=15` | stack sizes and task-watchdog timeout (no reason given in the file; the WROVER-KIT needed 8 KB main stack, this S3 runs with 6 KB) |
| `SPIRAM=y`, `SPIRAM_MODE_OCT`, `SPIRAM_SPEED_80M`, `SPIRAM_USE_MALLOC`, `SPIRAM_MALLOC_ALWAYSINTERNAL=4096`, `SPIRAM_MALLOC_RESERVE_INTERNAL=32768` | song data and the engine timeline of large songs go to PSRAM; small allocations and DMA (LCD band) stay internal |
| `SPI_FLASH_MMAP_PAGES_PRESERVED=n` | the song pack (~9 MB) is memory-mapped |
| `BT_ENABLED`, `BT_NIMBLE_ENABLED`, `BT_NIMBLE_MEM_ALLOC_MODE_EXTERNAL` | native BLE controller + NimBLE host, buffers in PSRAM |
| `BT_NIMBLE_ROLE_CENTRAL/OBSERVER=y`, `PERIPHERAL/BROADCASTER=n`, `MAX_CONNECTIONS=1` | the board only connects to the piano |
| `BT_NIMBLE_ATT_PREFERRED_MTU=260` | the piano offers ~500; writes are ≤ 244 bytes |
| `BT_NIMBLE_NVS_PERSIST=y`, `BT_NIMBLE_HOST_TASK_STACK_SIZE=8192`, `BT_NIMBLE_LOG_LEVEL_WARNING` | keep bonds; stack headroom; quiet logs |
| `FATFS_LFN_HEAP`, `FATFS_MAX_LFN=255`, `FATFS_USE_LABEL` | not for this board: the shared ESP-IDF checkout carries the exFAT patch, which needs LFN to compile; the linker drops FatFs |

Bench numbers (firmware doc): app 2.9 MB of 4 MB, 8.3 MB heap free.

## 6. Build, flash, console

From `firmware/piano-coach` (first build compiles ESP-IDF, 10+ minutes;
run it in the background):

```bash
tools/board.sh tdisplays3 build --release
tools/songs.sh tdisplays3 build                       # pack + boards/tdisplays3/glyphs.slint
tools/songs.sh tdisplays3 flash --port /dev/ttyACM0   # writes target/tdisplays3/songs.bin at 0x410000
tools/board.sh tdisplays3 run --release -- --port /dev/ttyACM0   # flash app + monitor (blocks)
```

* `tools/board.sh` sources `boards/tdisplays3/board.env` (FEATURE, MCU
  `esp32s3`, ESP_IDF_VERSION v5.5.3, TARGET `xtensa-esp32s3-espidf`,
  TOOLCHAIN `esp198`, FLASHER `espflash`, `UI=mini`, `HAS=` empty), runs
  `cargo +esp198 … --target xtensa-esp32s3-espidf --no-default-features
  --features board-tdisplays3`, finds esp-clang's libclang under the
  toolchain for bindgen, and builds into `target/tdisplays3` with
  `sdkconfig.tdisplays3`. `build.rs` turns `UI=mini` into `cfg(ui_mini)`.
* `run` uses `flash.sh` as the cargo runner: `espflash flash --monitor
  --bootloader … --partition-table … <elf>` with the same build's
  bootloader and partition table.
* Direct flash without monitor (as used in sessions):
  `espflash flash --chip esp32s3 --port /dev/ttyACMx --baud 921600 --bootloader T/bootloader.bin --partition-table T/partition-table.bin T/piano-coach`
  with `T=target/tdisplays3/xtensa-esp32s3-espidf/release`.
* `esp-idf-components.toml` only generates the bindings module `bsp` from
  `src/bindings.h` (NimBLE, esp_lcd, I2C APIs); there is no C BSP.
* **Port identification:** the port is `/dev/ttyACM*` (USB Serial/JTAG).
  Other boards (Tab5, round board, YD-ESP32-S3) also show up as
  `ttyACM*`; check with `udevadm info -q property -n /dev/ttyACMx | grep
  ID_SERIAL` or `espflash board-info --port …` (chip ESP32-S3, 16 MB) before
  flashing. The bench unit's MAC / serial was not recorded.
* **Download mode:** normally not needed — espflash resets the S3 into
  the ROM loader over USB Serial/JTAG. If other firmware has taken over
  the USB port (e.g. a "BLE USB Dongle" image: the PC sees a Bluetooth
  adapter and no serial port), hold BOOT, press and release RST, release
  BOOT; `/dev/ttyACM*` appears. Piano Coach keeps the USB Serial/JTAG
  console, so later flashes need no buttons. Press RST to leave download
  mode.
* **Console / reset:** open the console with `espflash monitor` (resets the
  board once). Do not use `espflash monitor --before no-reset`: it leaves
  the chip in the ROM bootloader. On native USB Serial/JTAG ports, opening
  the port from pyserial can reset the chip or put it into download mode;
  `espflash reset` recovers.
* **Setting a pref from the PC:** NVS is at 0x9000, 24 KB (`0x6000`);
  the procedure is in AGENTS.md ("set a pref from the PC"). It replaces
  all saved prefs and BLE bonds. This board has no `lcdmirror` pref.

## 7. Piano Coach support

* **UI class:** mini (`ui_mini.rs`, `ui/mini/app.slint`): library wheel,
  song menu (Learn / Follow / Listen, hands, tempo), play screen, settings
  (brightness, Sleep, …), 320×170 at scale 1.0. Two-key navigation via
  `Button::Nav{Next,Prev,Ok,Back}`; touch (if found) via `touch_point`:
  tap a row, swipe up/down to scroll, swipe right = back; on the play
  screen tap = pause / resume.
* **Caps:** name "LILYGO T-Display-S3", backlight yes, power latch no,
  USB-MIDI no, BLE-MIDI yes, buttons yes, touch yes (found at run time),
  wake hint "BOOT or RESET".
* **Transport:** BLE only; connects to `GZUT-MIDI` by itself. Only one
  central at a time: power off the Tab5 / tablets / other boards when
  testing. `bt_controller_start` and `init_power` are no-ops.
* **Sleep (`power_off`)**: backlight 0, wait until BOOT is released,
  GPIO15 low, ext0 wake on GPIO0 low, `esp_deep_sleep_start`. BOOT or RST
  wakes it (a full reboot).
* **Boot:** Follow mode with a random 儿时回忆 song that starts when the
  piano connects (all boards).

Verified on hardware (bench unit ESP32-S3R8 rev 0.2, no touch; commit
64a92f9, 2026-10-16): download mode via BOOT + RST (it ran a "BLE USB
Dongle" image), boot, display right way up and in colour with the vendor
init sequence at 16 MHz, 2449 songs mounted from the pack, BLE connects,
KEY / BOOT navigation and Learn ✔.

Not verified on this board: the touch variant (CST816 mapping, CST328
units); the piano-key remote control, shuffle-play and Follow (added after
the port, tried on the Tab5 and the ESP32-1732S019, not here); long
two-way BLE sessions; brightness steps; sleep current and wake from
battery; battery operation in general.

Known limits (all small boards): no search, bookmarks (C8 does nothing),
A–B loop, sheet view, speaker or song globe.

## 8. Quirks and lessons learned

* **GPIO15 first.** On battery nothing on V3V (backlight driver, LCD pin
  20) has power until GPIO15 is high; the firmware raises it in
  `early_init` before the LCD is touched. On USB power the board works
  without it, so a missing GPIO15 shows up only on battery.
* **RD must be held high** (GPIO9) — the i80 driver has no RD line.
* **Newer panels need the vendor ST7789 sequence** ("the product now has
  two screens", vendor comment on `LCD_MODULE_CMD_1`). Without it colours
  and gamma can be wrong; with it the bench panel is right.
* **35-pixel gap** on the 170-pixel axis; orientation swap XY + mirror Y
  gives landscape with USB on the left.
* **Pixel clock:** 16 MHz works; the vendor warns that too low or too
  high a clock causes "mosaic" artefacts.
* **i80 is Xtensa-only in this code** (`#[cfg(target_arch = "xtensa")]`
  in `spi_lcd.rs`): the ESP32-C3 has no i80 peripheral.
* **USB port hijack:** firmware that reconfigures the native USB (TinyUSB
  device images) removes the serial port; use BOOT + RST once.
* **Touch controller variants** (CST816 at 0x15 vs early CST328 at 0x1A)
  and the touch mapping disagreement with the vendor IDF example — see
  §4.4.
* **Battery ADC** reads garbage (>4.3 V, the charger output) without a
  battery and is only meaningful with USB unplugged (vendor).

## 9. Sources

In this repo:

* `firmware/piano-coach/src/board/tdisplays3/mod.rs` — pins, LCD init,
  touch mapping, keys, sleep.
* `firmware/piano-coach/src/board/spi_lcd.rs`, `src/board/cst816.rs`,
  `src/board/key.rs`, `src/board/mod.rs`, `src/board/contract.rs`.
* `firmware/piano-coach/boards/tdisplays3/` — `board.env`,
  `sdkconfig.defaults`, `partitions.csv`, `esp-idf-components.toml`,
  `glyphs.slint`.
* `firmware/piano-coach/build.rs`, `tools/board.sh`, `tools/songs.sh`,
  `flash.sh`, `.cargo/config.toml`, `src/songs.rs`, `src/ble_midi.rs`,
  `src/ui_mini.rs`.
* Generated `target/tdisplays3/xtensa-esp32s3-espidf/release/build/esp-idf-sys-*/out/sdkconfig`
  (flash 80 MHz, host flow control off) and `target/tdisplays3/songs.bin`
  (size).
* `AGENTS.md`, `doc/piano_coach_firmware.md` (small boards, build,
  "Verified on hardware"), `firmware/piano-coach/boards/README.md`,
  commit 64a92f9 message.
* [vendor/tdisplays3/T_Display_S3.pdf](vendor/tdisplays3/T_Display_S3.pdf)
  — LILYGO schematic (one sheet, 2024-04-29), copied from the vendor
  repository.

Outside the repo (LILYGO material, read for this document, not copied):

* `/home/nws/w/esp32/T-Display-S3/T-Display-S3/README.md` — product table,
  pinout, GPIO15 note, FAQ (download mode), LEDs, antenna, electrical
  parameters.
* `…/T-Display-S3/image/T-DISPLAY-S3.jpg` — pin-map photo (button
  positions, header labels).
* `…/T-Display-S3/examples/*/pin_config.h` (identical pin sets),
  `examples/lv_demos/lv_demos.ino` (esp_lcd i80 setup, init sequence),
  `examples/GetBatteryVoltage/GetBatteryVoltage.ino` (battery ADC),
  `examples/factory/pin_config.h` (16 MHz pixel clock),
  `examples/CapacitiveTouch/CapacitiveTouch.ino` and
  `examples/touch_test/touch_test.ino` (CST328 / CST816),
  `examples/Sleep/Sleep.ino`.
* `…/T-Display-S3/lib/SensorLib/src/REG/CSTxxxConstants.h` (CST816 0x15,
  CST328 0x1A).
* `…/T-Display-S3/datasheet/CST816S寄存器说明-20190508.pdf` (registers
  0x02–0x06, 0xFE). Also present but not needed here: `ST7789V_SPEC_V1.4.pdf`,
  `CST328数据手册V2.2.pdf`.
* `/home/nws/w/esp32/LilyGo-Display-IDF/` — `sdkconfig.defaults.t-display-s3`,
  `main/product_pins.h`, `main/display_s3.c` (10 MHz, orientation
  comment), `main/touch_driver.cpp` and
  `components/SensorLib/src/TouchDrvInterface.cpp` (touch transform).
  `LilyGo-Display-IDF2` / `IDF3` are copies with the same T-Display-S3
  files.
* ESP-IDF v5.5.3 `components/esp_lcd/src/esp_lcd_panel_st7789.c` (what
  `esp_lcd_panel_init` sends).
