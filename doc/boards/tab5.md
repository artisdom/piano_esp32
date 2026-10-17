# M5Stack Tab5 — hardware reference for Piano Coach

Self-contained reference for developing Piano Coach on the M5Stack Tab5.
Facts are tagged by where they come from:

* **code** — the firmware in this repo (what is known to work);
* **sch** — the Tab5 schematic (the five schematic pages and the block
  diagram embedded in `doc/M5Stack-Tab5/Tab5.pdf`, pages 8–11);
* **vendor** — M5Stack's product page (`doc/M5Stack-Tab5/tab5.md`, same
  content as the text of `Tab5.pdf`) or the vendored Espressif BSP's
  README;
* **doc** — this repo's own docs (`AGENTS.md`,
  `doc/piano_coach_firmware.md`, commit messages).

Anything the firmware does not exercise is marked "not used by firmware".
Where sources disagree, the disagreement is stated and the code wins for
what works.

## 1. Summary

The Tab5 is a 5″ tablet-style dev board: an ESP32-P4 application processor
(dual-core RISC-V, no radio) with a 720×1280 MIPI-DSI touch panel,
microSD, an ES8388 codec with a speaker amplifier, a USB-A host port, a
USB-C port and an ESP32-C6 coprocessor that provides Wi-Fi/Bluetooth over
SDIO (ESP-Hosted). Piano Coach runs its full UI on it (landscape, rendered
rotated into the portrait panel), reads songs from microSD, and reaches the
piano either over USB-A (USB-MIDI host) or over Bluetooth LE through the
C6 — one of the two per boot.

| Item | Value | Source |
|---|---|---|
| Product | M5Stack Tab5, SKU C145 (Kit with NP-F550 battery: K145) | vendor |
| Schematic revision | sheet images are named `sch_tab5_b08_page_0N` (presumably board rev "b08"; no revision text read on the sheets) | vendor |
| Panel generations | v1: ILI9881C + GT911 (until 2025-10-14); v2: ST7123 integrated display+touch (from 2025-10-14); ST7121 (from 2026-04-28) | vendor; v1/v2 handled by BSP (code) |
| Bench unit | ESP32-P4 rev v1.3, ST7123 panel, C6 ESP-Hosted slave 2.12.11 | doc |
| Board dir | `firmware/piano-coach/boards/tab5` | code |
| Cargo feature | `board-tab5` (also the crate's `default` feature) | code |
| MCU | ESP32-P4NRW32 (QFN104; 32 MB PSRAM in package), CPU 360 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ=360`) | vendor, P4 datasheet; code (generated sdkconfig) |
| Coprocessor | ESP32-C6-MINI-1U module (U2), Wi-Fi 6 / BLE, over SDIO | sch, vendor |
| CPU arch / Rust target | RISC-V `riscv32imafc-esp-espidf`, Rust nightly + `rust-src` (`rust-toolchain.toml`), `build-std` | code |
| ESP-IDF | v5.5.3 (`ESP_IDF_VERSION=v5.5.3`, fetched by embuild) | code |
| Flash | 16 MB, XM25QH128CHIQ (U3), DIO 80 MHz in the generated sdkconfig | sch, code |
| PSRAM | 32 MB in package, 200 MHz, hex mode in the generated sdkconfig (`CONFIG_SPIRAM_MODE_HEX=y`); vendor table says "32MB Octal" | code, vendor |
| Crystal | 40 MHz (X1) | sch |
| USB on Linux (USB-C) | Espressif USB Serial/JTAG, `303a:1001`, `/dev/ttyACM*` | doc (AGENTS.md) |
| Bench-unit MAC | `30:ed:a0:e2:fa:9b` | doc (AGENTS.md) |

## 2. Feature overview

| Component | Part | Function | Bus | Address / pins | Used by Piano Coach |
|---|---|---|---|---|---|
| Application SoC | ESP32-P4NRW32 (U1) | main CPU | — | — | yes |
| Radio coprocessor | ESP32-C6-MINI-1U (U2) | BLE controller (Wi-Fi unused) | SDIO 4-bit + reset | GPIO8–13, reset GPIO15 | yes (BLE mode) |
| Flash | XM25QH128CHIQ (U3) | 16 MB SPI NOR | flash SPI | dedicated pins | yes |
| IO expander 1 | PI4IOE5V6408 (U6) | rails/resets/amp enable | I²C | 0x43 (ADDR to GND) | yes |
| IO expander 2 | PI4IOE5V6408 (U7) | C6 power, USB-A 5 V, power-off pulse, charger | I²C | 0x44 (ADDR to 3V3) | yes |
| Display + touch (v2) | ST7123 TDDI | 720×1280 panel + touch | MIPI-DSI 2-lane; touch I²C | touch 0x55, INT GPIO23 | yes |
| Display (v1) | ILI9881C | 720×1280 panel | MIPI-DSI 2-lane | — | supported by BSP, not tried |
| Touch (v1) | GT911 | touch | I²C | 0x14 | supported by BSP, not tried |
| Display (2026+) | ST7121 | panel + touch | MIPI-DSI; I²C | 0x55 (vendor) | unknown — BSP has no ST7121 branch |
| Backlight driver | ME2212 (U8) boost | LED backlight | PWM on EN | GPIO22 | yes (LEDC PWM) |
| Audio DAC/ADC | ES8388 (U14) | speaker/headphone DAC | I²S + I²C | 7-bit 0x10 (`ES8388_CODEC_DEFAULT_ADDR` = 0x20 in esp_codec_dev's 8-bit form) | yes (DAC only) |
| Mic ADC | ES7210 (U13) | 4-ch mic ADC / AEC | I²S + I²C | 7-bit 0x40 (0x80 8-bit form) | no |
| Microphones | MSM381A3729H9BPC ×2 (U16, U17) | dual mic | analog to ES7210 | — | no |
| Speaker amp | NS4150B (U15) | 1 W mono class-D | analog from ES8388 LOUT1 | enable: expander 0x43 P1 | yes |
| Headphone jack | PJ-342 (J5) | 3.5 mm | analog | detect: expander 0x43 P7 | output shares the codec; detect not read |
| microSD | TF socket (J2) | song library | SDMMC slot 0, 4-bit | GPIO39–44 | yes |
| USB-A | J10 | host port for the piano | P4 USB 2.0 HS OTG (dedicated pins) | VBUS: expander 0x44 P3 | yes (USB mode) |
| USB-C | J8 | power in, console, flashing | P4 USB Serial/JTAG (GPIO24/25) | — | yes |
| USB-UART bridge footprint | CH9102F (U32) | marked "U32/NC" | — | — | not fitted (sch) |
| Charger | IP2326 (U20) | 2-cell charger | — | CHG_EN 0x44 P7 | enabled at boot |
| Power monitor | INA226 (U31) | battery current/voltage | I²C | 0x41 (vendor) | no |
| RTC | RX8130CE (U30) | RTC, wake interrupt | I²C | 0x32 (vendor) | no |
| IMU | BMI270 (U19) | 6-axis | I²C (level-shifted) | 0x68 (SDO to GND) | no |
| Power controller | PMS150G-U06 (U28), "custom-programmed chip" | power button, main power enable, wake | GPIO-level | — | indirectly (power-off pulse) |
| Camera | SC2356 per vendor (BSP says SC202CS) | 2 MP | MIPI-CSI 2-lane | MCLK GPIO36, reset 0x43 P6 | no |
| RS-485 | SIT3088 (U18) | RS-485 transceiver | UART | TX GPIO20, RX GPIO21, DIR GPIO34 | no |
| Expansion | M5-Bus, HY2.0-4P (Grove), 2×5 header, STAMP pads | — | — | see §3 | no |
| Antenna switch | RF switch U29 | internal 3D antenna vs MMCX | — | 0x43 P0 | driven low (internal antenna) |

## 3. Pin map

### 3.1 ESP32-P4 GPIOs

"Used" means the Tab5 Piano Coach build touches the pin (directly or through
the BSP / ESP-Hosted).

| GPIO | Schematic net / function | Dir / level | Peripheral | Used | Source |
|---|---|---|---|---|---|
| 0, 1 | STD_GPIO0/1 (XTAL_32K_N/P pins; 2×5 header) | — | — | no | sch |
| 2, 3, 4 | STD_GPIO2/3/4 (M5-Bus G2/G3/G4) | — | — | no | sch, vendor |
| 5 | xSPI_SCK_GPIO5 (M5-Bus SCK) | — | — | no | sch, vendor |
| 6 | xMOD_RX_GPIO6 (M5-Bus pin 16 "PC_TX" in vendor table) | — | — | no | sch, vendor |
| 7 | STD_GPIO7 (M5-Bus pin 15 "PC_RX") | — | — | no | sch, vendor |
| 8 | SDIO2_D3 → C6 IO23 | bidir | SDIO slot 1 | yes (BLE mode) | code (sdkconfig), sch |
| 9 | SDIO2_D2 → C6 IO22 | bidir | SDIO slot 1 | yes (BLE mode) | code, sch |
| 10 | SDIO2_D1 → C6 IO21 | bidir | SDIO slot 1 | yes (BLE mode) | code, sch |
| 11 | SDIO2_D0 → C6 IO20 | bidir | SDIO slot 1 | yes (BLE mode) | code, sch |
| 12 | SDIO2_CLK → C6 IO19 | out | SDIO slot 1, 25 MHz | yes (BLE mode) | code, sch |
| 13 | SDIO2_CMD → C6 IO18 | bidir | SDIO slot 1 | yes (BLE mode) | code, sch |
| 14 | RF_C6_IO2 → C6 IO2 | — | — | no | sch, vendor |
| 15 | SOC_EXTRF_RST → C6 EN (via 1 kΩ; 10 kΩ pull-up to WLAN_3.3V, 1 µF) | out | ESP-Hosted slave reset (`CONFIG_ESP_HOSTED_SDIO_GPIO_RESET_SLAVE=15`, `CONFIG_ESP_HOSTED_SDIO_RESET_ACTIVE_HIGH=y`) | yes (BLE mode) | code, sch |
| 16, 17 | STD_GPIO16/17 (M5-Bus G16, G17 "PB_IN") | — | — | no | sch, vendor |
| 18, 19 | xSPI_MOSI_G18 / xSPI_MISO_G19 (M5-Bus) | — | — | no | sch, vendor |
| 20 | RS485_TX_GPIO20 | — | — | no | sch, vendor (see §8: BSP header calls it `BSP_USB_POS`) |
| 21 | RS485_RX_GPIO21 | — | — | no | sch, vendor |
| 22 | LCD_BL_GPIO22 → ME2212 EN (backlight boost) | out, PWM, high = on | LEDC low-speed ch 1, timer 0, 5 kHz, 10-bit | yes | code (BSP), sch |
| 23 | TP_INT_GPIO23 (5.1 kΩ pull-up R92) | in, active low | touch interrupt (ST7123); v1/GT911: driven low as output | yes | code (BSP), sch |
| 24 | USB_DEVICE_DM → USB-C D− (33 Ω + choke) | — | USB Serial/JTAG (FS PHY) | yes (console, flashing) | sch |
| 25 | USB_DEVICE_DP → USB-C D+ | — | USB Serial/JTAG | yes | sch |
| 26 | I2S_DIN_MOSI_GPIO26 → ES8388 DSDIN | out | I²S1 DOUT | yes | code (BSP), sch |
| 27 | I2S_SCLK_GPIO27 → ES8388 + ES7210 SCLK | out | I²S1 BCLK | yes | code, sch |
| 28 | I2S_ASDOUT_MISO_GPIO28 ← ES7210 SDOUT | in | I²S1 DIN | configured by BSP, data unused | code, sch |
| 29 | I2S_LRCK_GPIO29 → both codecs | out | I²S1 WS | yes | code, sch |
| 30 | I2S_MCLK_GPIO30 → both codecs | out | I²S1 MCLK | yes | code, sch |
| 31 | xG31_SYS_SDA (2.2 kΩ pull-up R93) | bidir, open drain | I²C port 1 (`CONFIG_BSP_I2C_NUM=1`) | yes | code, sch |
| 32 | xG32_SYS_SCL (2.2 kΩ pull-up R76) | out, open drain | I²C port 1 | yes (also bit-banged by `i2c_bus_clear`) | code, sch |
| 33 | MOD_SPI_CS_G33 (STAMP pad TP6) | — | — | no | sch |
| 34 | RS485_DIR_GPIO34 (strapping pin) | — | — | no | sch, vendor |
| 35 | BOOT_GPIO35 (strapping; 10 kΩ pull-up R9; also to U28 PA5) | in at reset | boot mode strap | no (strap only) | sch |
| 36 | CAM_MCLK_GPIO36 (strapping; 10 kΩ pull-up R10) | — | camera MCLK | no | sch, vendor |
| 37 | U0TXD_GPIO37 (strapping; M5-Bus TXD0) | out | UART0 TX — primary console (`CONFIG_ESP_CONSOLE_UART_NUM=0`, 115200) | yes (console) | code, sch, vendor |
| 38 | U0RXD_GPIO38 (strapping; M5-Bus RXD0) | in | UART0 RX | yes (console) | code, sch, vendor |
| 39 | SDIO1_D0 (5.1 kΩ pull-up) | bidir | SDMMC slot 0 D0 | yes | code (BSP), sch |
| 40 | SDIO1_D1 | bidir | SDMMC D1 | yes | code, sch |
| 41 | SDIO1_D2 | bidir | SDMMC D2 | yes | code, sch |
| 42 | SDIO1_D3 | bidir | SDMMC D3 | yes | code, sch |
| 43 | SDIO1_SCK (22 Ω R23) | out | SDMMC CLK | yes | code, sch |
| 44 | SDIO1_CMD | bidir | SDMMC CMD | yes | code, sch |
| 45–52 | STD_GPIO45…52 (M5-Bus / headers) | — | — | no | sch, vendor |
| 53, 54 | STD_GPIO53/54 → HY2.0-4P (Grove) yellow/white | — | — | no | sch, vendor |
| USB2_OTG_D−/D+ (pins 49/50, dedicated) | USB_HOST_DM/DP → USB-A J10 (33 Ω + choke) | — | USB 2.0 HS OTG (host) | yes (USB mode) | sch, code |
| DSI_CK±, DSI_D0±, DSI_D1± (dedicated) | LCD FPC J3 | — | MIPI-DSI, 2 lanes | yes | sch, code |
| CSI_CK±, CSI_D0±, CSI_D1± (dedicated) | camera FPC J4 | — | MIPI-CSI | no | sch |
| CHIP_PU | SOC_RST (10 kΩ pull-up R11, 1 µF C35); also nRESET of both expanders | in | chip enable | — | sch |

### 3.2 System I²C bus (SDA GPIO31, SCL GPIO32)

One shared bus: `bsp_i2c_init()` creates it on I²C port 1 with internal
pull-ups enabled (external 2.2 kΩ pull-ups exist too). The firmware adds the
expander devices at 400 kHz. Addresses are 7-bit.

| Addr | Device | Used | Source |
|---|---|---|---|
| 0x10 | ES8388 (CE tied to AGND) | yes | sch, vendor, code (8-bit 0x20) |
| 0x14 | GT911 (v1 panels; `ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP`) | probed at boot | code |
| 0x32 | RX8130CE | no | vendor |
| 0x40 | ES7210 (AD0/AD1 to AGND) | no | sch, vendor |
| 0x41 | INA226 | no | vendor |
| 0x43 | PI4IOE5V6408 U6 (`..._ADDRESS_LOW`) | yes | code, sch, vendor |
| 0x44 | PI4IOE5V6408 U7 (`..._ADDRESS_HIGH`) | yes | code, sch, vendor |
| 0x55 | ST7123 touch (`ESP_LCD_TOUCH_IO_I2C_ST7123_ADDRESS`); ST7121 also 0x55 per vendor | yes (probed first) | code, vendor |
| 0x68 | BMI270 (SDO to GND; on the bus through a 2N7002DW level shifter) | no | sch, vendor |
| ? | camera sensor SCCB | no | unknown |

### 3.3 IO expander bits

Register addresses as named in `src/board/tab5/mod.rs`: `0x03` IO_DIR
(1 = output), `0x05` OUT_SET, `0x07` OUT_HIGH_Z, `0x09` IN_DEF_STA,
`0x0B` PULL_EN, `0x0D` PULL_SEL, `0x11` INT_MASK. Both expanders' nRESET
pins are on SOC_RST (sch); their nINT pins show no net (sch).

**0x43 (U6)**

| Bit | Schematic net | Function | Firmware use | BSP name |
|---|---|---|---|---|
| P0 | RF_PTH_L_INT_H_EXT | antenna path: low = internal 3D antenna, high = external MMCX (vendor) | output, low | — (`BSP_WIFI_EN` is on 0x44) |
| P1 | SPK_EN | NS4150B CTRL (10 kΩ pull-down R45), high = amp on | output; low at boot, toggled around playback | `BSP_SPEAKER_EN` |
| P2 | EXT5V_EN | enables U26 → SYS_EXT5VO (M5-Bus, side header, Grove 5 V) | output, low (off) | — |
| P3 | (no net) | — | output, low | — |
| P4 | LCD_RST | panel reset, high = run | output, high; pulsed low in `display_init` | `BSP_LCD_EN` |
| P5 | TP_RST | touch reset, high = run | output, high | `BSP_TOUCH_EN` |
| P6 | CAM_RST | camera reset | output, high | `BSP_CAMERA_EN` |
| P7 | HP_DET | headphone detect | input (not read) | — |

`init_power` writes: IO_DIR `0b0111_1111`, OUT_HIGH_Z `0`, PULL_SEL
`0b0111_1111`, PULL_EN `0b0111_1111`, OUT_SET `0b0111_0000` ("LCD/TP/CAM
out of reset").

**0x44 (U7)**

| Bit | Schematic net | Function | Firmware use | BSP name |
|---|---|---|---|---|
| P0 | WLAN_PWR_EN | enables U38 (LPW5209) → WLAN_3.3V, the C6's supply | output: high in BLE mode, low in USB mode | `BSP_WIFI_EN` |
| P1 | (no net) | — | high-Z | — |
| P2 | (no net) | — | high-Z | — |
| P3 | USB5V_EN | enables U27 (MT9700) → SYS_USB5V = USB-A VBUS | output: high in USB mode, low in BLE mode | `BSP_USB_EN` |
| P4 | PWROFF_PLUSE (sic; firmware: POWEROFF_GEN) | power-off pulse into the power-latch circuit (via D12) | output, low; 3 pulses in `power_off` | — |
| P5 | nCHG_QC_EN | switches a resistor on the IP2326 NTC pin through Q1 (sch); code comment: "QC_CHG_EN (active low)" | output, low | — |
| P6 | CHG_STAT | IP2326 LED/status output (sch, vendor "CHG_STAT_LED"); code comment says "USB-C detect" — see §8 | input; IN_DEF_STA bit set, the only unmasked interrupt | — |
| P7 | CHG_EN | IP2326 EN (10 kΩ pull-down R59), high = charging enabled | output, high | — |

`init_power` writes: IO_DIR `0b1011_1001`, OUT_HIGH_Z `0b0000_0110`,
PULL_SEL `0b1011_1001`, PULL_EN `0b1011_1001`, IN_DEF_STA `0b0100_0000`,
INT_MASK `0b1011_1111`, OUT_SET `0b1000_1000` (USB mode: CHG_EN +
USB5V_EN) or `0b1000_0001` (BLE mode: CHG_EN + WLAN_PWR_EN).

### 3.4 Connectors (not used by firmware)

* **M5-Bus** (vendor table): 1/3/5 GND, 2 G16, 4 G17 (PB_IN), 6 RST (EN),
  7 G18 MOSI, 8 G45, 9 G19 MISO, 10 G52 (PB_OUT), 11 G5 SCK, 12 3V3,
  13 G38 RXD0, 14 G37 TXD0, 15 G7 PC_RX, 16 G6 PC_TX, 17 G31 SDA,
  18 G32 SCL, 19 G3, 20 G4, 21 G2, 22 G48, 23 G47, 24 G35, 25/27/29
  HVIN, 26 G51, 28 5V, 30 BAT. Pins 17/18 are the **system** I²C bus.
* **HY2.0-4P (Grove, J11)**: GND, 5 V (SYS_EXT5VO, switched by 0x43 P2),
  G53, G54.
* **RS-485 (J7, 1.25 mm 6P)**: SIT3088, TX G20, RX G21, DIR G34; vendor:
  6–24 V supply input, switchable 120 Ω termination.
* **2×5 header (J9)**, **STAMP pads**, **C6 ISP header J1** (3V3, C6
  TXD0, C6 RXD0, C6 EN, C6 IO9, GND).

## 4. Subsystems

### 4.1 Power, battery, charging

Power tree as drawn (sch, page 5):

* Inputs: USB-C VBUS (USB_5VIN, 2 A resettable fuse), the RS-485/HVIN
  supply (SYS_VIN → MP4560 buck → PRE_5V), and a 2-cell battery
  (NP-F550, 7.4 V 2000 mAh; SYS_BAT_2S). Two MAX40200 ideal diodes merge
  PRE_5V and USB_5VIN into SYS_5VBUS.
* SYS_5VBUS → U23 LPW5209 (EN = **MPWR_EN** from U28) → **SYS_INT5V**.
  From the battery: SYS_BAT_2S → U24 SY8113 (EN = MPWR_EN) → SYS_BCK →
  P-FET Q9 → SYS_INT5V; Q9 is switched through Q10 by VBK_LCK, which U37
  (SSP61CC3002MR) derives from PRE_5V/USB_5VIN, i.e. from the presence of
  external 5 V. VDD_STBY (U28, RTC) comes from an ME6239 LDO (U25) fed
  from SYS_5VBUS or the battery through diodes. The charger input
  SYS_5VCHG is PRE_5V or USB_5VIN through Schottky diodes.
* SYS_INT5V → U4 SY8089 buck → **SOC_3.3V** (P4, card, panel logic).
  SYS_INT5V also feeds the backlight boost, the NS4150B, U26 (EXT5V) and
  U27 (USB-A VBUS).
* **U28 PMS150G-U06** ("custom-programmed chip", on VDD_STBY) reads the
  side power button S1 (SW_PWR), drives MPWR_EN and BOOT_GPIO35, has the
  green LED1, and takes nINT_STAT_TRG. The PWROFF_PLUSE line from
  expander 0x44 P4 and the wake sources (E_TRG: BMI270 INT1, RX8130CE
  IRQ) act on this latch circuit through Q3/Q4. Its firmware is not
  available; behaviour beyond the wiring is from vendor text and testing.
* Charger IP2326 (U20): EN = CHG_EN (0x44 P7), status = CHG_STAT
  (0x44 P6). Vendor: "Tab5 can only be charged after the device is powered
  on and initialized" — so the firmware must set CHG_EN (it does, in both
  transports). Over-discharged packs (< 6 V) need the vendor's recovery
  procedure (`doc/M5Stack-Tab5/tab5.md`, "Tab5 Charging").
* INA226 monitors the battery path (sch); not read by firmware.
* Vendor notes: power on = one press of the power button; shut down =
  double press (vendor UI behaviour; whether the PMS150 or the app
  handles the double press is unknown). After pulling power, wait 5 s
  before re-powering or the IMU may fail to initialise.

**Firmware shut-down** (`power_off`, code): backlight 0, BSP features
LCD/USB/WIFI off, wait 120 ms, then three 100 ms high/low pulses on
0x44 P4 ("POWEROFF_GEN", per M5Tab5-UserDemo `bsp_generate_poweroff_signal`
and herdline). If the CPU is still running (an external supply holds the
rail — USB-C connected), it logs "still powered (external supply) —
entering deep sleep" and calls `esp_deep_sleep_start()` with no wake
source configured; the reset or power button restarts it.
`CAPS.power_latch = true`, `wake_hint = "the side power button"`.

### 4.2 USB ports

| Port | Wiring (sch) | Role | Notes |
|---|---|---|---|
| USB-C (J8) | VBUS → USB_5VIN (power in); D± → P4 GPIO24/25 (USB Serial/JTAG); CC1/CC2 5.1 kΩ pull-downs (sink); also brought to a 1.25 mm 4-pin header J6 | console + flashing + power | `303a:1001`, `/dev/ttyACM*`. Vendor calls this port "USB 2.0 OTG"; on this schematic its data lines go to the FS USB Serial/JTAG pins, and the CC pull-downs make it a device/sink port |
| USB-A (J10) | D± → P4 USB2_OTG HS pins via 33 Ω + choke; VCC = SYS_USB5V from U27 MT9700 (EN = 0x44 P3) | **host** for the piano | 5 V is on only in USB mode |

USB mode (code): `init_power(Usb)` sets USB5V_EN; `usb_host_start()` calls
`bsp_usb_host_start(BSP_USB_HOST_POWER_MODE_USB_DEV, false)`, which enables
`BSP_FEATURE_USB` again, installs the IDF USB host library (HS PHY,
`skip_phy_setup = false`, `ESP_INTR_FLAG_LOWMED`) and starts the
`usb_lib` task (4096 B stack, priority 10). The piano is bus-powered and
detects the host by VBUS (code comment). The power-mode and 500 mA
arguments are "Not used on this board" (BSP header).

`src/usb_midi.rs`: claims the Audio/MIDI-Streaming interface (class 1,
subclass 3), bulk endpoints (piano: IN 0x82 / OUT 0x02, MPS 64), one IN
transfer always queued, OUT at most 8 events per transfer and 12 ms apart
under backlog, key events in the first 500 ms after attach dropped
(`STALE_MS`).

### 4.3 Display

| Item | Value | Source |
|---|---|---|
| Panel | 5″ IPS 720×1280 portrait; ST7123 on the bench unit | vendor, doc |
| Interface | MIPI-DSI, 2 data lanes, 1000 Mbps per lane (`cfg.dsi_bus.lane_bit_rate_mbps = 1000`) | code |
| DSI PHY supply | on-chip LDO channel 3 at 2500 mV (`BSP_MIPI_DSI_PHY_PWR_LDO_CHAN`) → VDD_MIPI_DPHY | code, sch (ESP_LDO3 on pin 41) |
| Command IO | DBI over DSI, virtual channel 0, 8-bit commands/params | code (BSP) |
| DPI (ST7123) | 70 MHz pixel clock; HSYNC bp 40 / pw 2 / fp 40; VSYNC bp 8 / pw 2 / fp 220 | code (BSP) |
| DPI (ILI9881C) | 60 MHz; HSYNC bp 140 / pw 40 / fp 40; VSYNC bp 20 / pw 4 / fp 20 | code (BSP) |
| Pixel format | RGB565, RGB element order, little-endian (`BSP_LCD_BIGENDIAN 0`) | code |
| Framebuffers | 2 DPI framebuffers (`CONFIG_BSP_LCD_DPI_BUFFER_NUMS=2`), in PSRAM; DMA2D enabled (`CONFIG_BSP_LCD_USE_DMA2D=y`, IDF < 6) | code |
| Panel reset | no GPIO (`BSP_LCD_RST = NC`); reset is expander 0x43 P4 (LCD_RST) | code, sch |
| Orientation | panel left un-mirrored, no swap; UI rendered with `RenderingRotation::Rotate270` | code |
| Backlight | ME2212 boost from SYS_INT5V, EN = GPIO22 PWM: LEDC low-speed, timer 0, channel 1, 5 kHz, 10-bit, duty = 1023 × % / 100 | code (BSP), sch |

Init sequence (`display_init`, code):

1. `bsp_i2c_init()`.
2. `bsp_feature_enable(LCD, false)`, 400 ms, `(LCD, true)`, 250 ms — the
   code calls this a power cycle ("a warm reset can leave the panel half
   initialised and the DSI link will not re-train over that"); on the
   schematic the line is LCD_RST, so it is a long reset pulse (§8).
3. `bsp_display_new()`: LCD feature on, backlight LEDC init, DSI PHY LDO,
   DSI bus, DBI IO, probe of the panel generation (`bsp_get_board_version`:
   enables TP_RST, waits 500 ms, probes 0x55 → ST7123 "version 2", else
   0x14 → ILI9881C/GT911 "version 1", else logs "Unsupported board
   version!" and asserts), vendor init table, reset, init, invert off,
   mirror off.
4. `esp_lcd_panel_disp_on_off(true)`, `bsp_display_brightness_init()`,
   brightness 80 %.
5. `bsp_touch_new()`, then both framebuffers via
   `esp_lcd_dpi_panel_get_frame_buffer(panel, 2, …)`, filled with the
   background and written back (`adopt`), then `DISPLAY_UP` is set.

The ST7123 init table (`priv_include/disp_init_data_1.h`) carries a
herdline patch: SLPOUT (0x11) is followed by 120 ms and DISPON (0x29) by
50 ms. Quoting the comment: "The upstream table used delay 0 for both, so
DISPON raced the analog bring-up: random washed-out 'white hue' boots".

Geometry and presentation (code):

* `FRAME_WIDTH = 720`, `FRAME_HEIGHT = 1280` (as scanned);
  `WINDOW_WIDTH = 1280`, `WINDOW_HEIGHT = 720`, `UI_SCALE = 1.0`.
* `fb_index(x, y) = (1279 − x) × 720 + y` (landscape x runs up the panel,
  landscape y across it).
* `present()`: rounds the dirty band to even rows (one row = 1440 B, two
  rows = 2880 B = 45 × 64, as `esp_cache_msync` needs 64-byte alignment),
  writes it back with `esp_cache_msync(..., C2M)`, then
  `esp_lcd_panel_draw_bitmap(panel, 0, 0, 720, 1280, fb[back])`, which
  switches scanout to that buffer.
* Brightness: Settings slider 10–100 % (NVS key `brightness`, default 80).

### 4.4 Touch

| Item | Value | Source |
|---|---|---|
| Controller | ST7123 integrated touch (v2) at 0x55; GT911 at 0x14 on v1 | code |
| INT | GPIO23 (pull-up R92 5.1 kΩ), configured as `int_gpio_num` for ST7123 | code, sch |
| RST | expander 0x43 P5 (TP_RST); `rst_gpio_num = NC` | code, sch |
| Config | `x_max = 720`, `y_max = 1280`, no swap/mirror | code |
| Mapping | raw (x, y) → logical `(1279 − y, x)` (`touch_to_logical`) | code |
| Polling | `esp_lcd_touch_read_data` + `get_coordinates`, one point, from the UI loop | code |

v1 boards: the BSP drives GPIO23 low as an output and passes no INT pin
("there is resistor to 3V3 on interrupt pin which is blocking GT911
touch"). Not tried with Piano Coach.

### 4.5 Buttons

* `CAPS.buttons = false`; `poll_buttons()` returns nothing. The UI is
  touch-only; the piano's control keys act as a remote (shared code).
* Schematic: one button, S1 (SW_PWR), wired to U28 (PMS150), not to a P4
  GPIO. The vendor spec table lists one "Reset/Boot Button … for power
  on/off and entering download mode"; the vendor's download-mode text
  calls it the reset button (hold ~2 s until the green LED flashes
  rapidly). That U28 generates the P4 reset/BOOT strap for this is an
  inference from the wiring (U28 PA5 → BOOT_GPIO35), not verified.

### 4.6 LEDs

* LED1 (green) on VDD_STBY, driven by U28 (sch). Vendor: flashes rapidly
  in download mode. Not controllable from the P4.
* No user LED (`BSP_CAPS_LED 0`). Charger status goes to expander 0x44 P6.

### 4.7 Audio

| Item | Value | Source |
|---|---|---|
| Codec | ES8388 at 0x10, DAC mode, I²S slave (`master_mode = false`), `pa_pin = NC`, `hw_gain` PA 5.0 V / DAC 3.3 V | code (BSP) |
| I²S | port 1 (`CONFIG_BSP_I2S_NUM=1`), master, full duplex std Philips, 16-bit, mono slot; BSP default 48 kHz, opened by the firmware at 24 kHz | code |
| I²S pins | MCLK 30, BCLK 27, WS 29, DOUT 26 (→ ES8388 DSDIN), DIN 28 (← ES7210) | code, sch, vendor |
| Analog path | ES8388 LOUT1 → HPOUT_L, ROUT1 → HPOUT_R (headphone jack); NS4150B INP ← HPOUT_L (100 nF + 68 kΩ), INN ← AGND. LOUT2/ROUT2 are not connected | sch |
| Amp | NS4150B, VCC = SYS_INT5V, CTRL = SPK_EN (0x43 P1); speaker on TP1/TP13; vendor: 1 W @ 8 Ω | sch, vendor |
| Firmware format | `AUDIO_OUT_RATE = 24_000`, `AUDIO_CHANNELS = 1`, `SYNTH_GAIN = 0.8` | code |

Flow (code): `speaker_codec()` waits for `DISPLAY_UP` (the codec is on the
I²C bus the display bring-up creates), then `bsp_audio_init(NULL)` and
`bsp_audio_codec_speaker_init()` (which also enables the speaker feature).
`audio.rs` runs on core 1 at priority 12 with a 12 KB stack, renders
256-sample blocks, opens the codec and sets `SPEAKER` on only while voices
sound and closes both after 2 s of silence ("an open, idle amp hisses").
Volume: `esp_codec_dev_set_out_vol`, 0–100, NVS key `spkvol`, default 70.
The headphone output carries the same signal (both on LOUT1/ROUT1);
HP_DET is not read, so the speaker is not muted when headphones are in.
The ES7210 microphones are not used.

### 4.8 Storage (microSD)

| Item | Value | Source |
|---|---|---|
| Host | SDMMC slot 0, 4-bit, `SDMMC_FREQ_HIGHSPEED`, no CD/WP | code (BSP) |
| Pins | CLK 43, CMD 44, D0 39, D1 40, D2 41, D3 42 | code, sch, vendor |
| SPI-mode alternative (BSP, unused) | SPI3: CLK 43, MOSI 44, MISO 39, CS 42 | code |
| Card power | card VDD = SOC_3.3V (sch); the BSP still attaches an on-chip-LDO power control on channel 4 (`sd_pwr_ctrl_new_on_chip_ldo`, `ldo_chan_id = 4`) | sch, code |
| Pull-ups | 5.1 kΩ on CMD/D0–D3, ESD arrays D1/D2 | sch |
| Card detect | socket pin 9 "Detect" not connected | sch |
| Mount | `/sdcard`, FAT, `max_files = 5`, no format on failure, 16 KB allocation unit | code |
| FatFs | LFN on heap, 255 chars, UTF-8 API, no per-file cache, labels on, `VFS_MAX_COUNT=8` | code (sdkconfig) |

Library root: `/sdcard/midi` (or `/sdcard/MIDI`), else `/sdcard`; the
card's single `midi/JustPiano2-20160326/` folder is stepped into
(`browse.rs`). Progress/bookmarks: `/sdcard/.pianocoach/progress.tsv`.
A card inserted after boot is mounted by Rescan in the UI.

### 4.9 Radio (ESP32-C6 over ESP-Hosted)

| Item | Value | Source |
|---|---|---|
| Module | ESP32-C6-MINI-1U (U2), supply WLAN_3.3V from U38 LPW5209 enabled by WLAN_PWR_EN (0x44 P0) | sch |
| Link | SDIO slot 1, 4-bit, 25 MHz (`CONFIG_ESP_HOSTED_SDIO_CLOCK_FREQ_KHZ=25000`) | code |
| P4 pins | CLK 12, CMD 13, D0 11, D1 10, D2 9, D3 8, reset 15 | code, sch, vendor |
| C6 pins | CLK IO19, CMD IO18, D0 IO20, D1 IO21, D2 IO22, D3 IO23 (22 Ω series, 5.1 kΩ pull-ups); IO8/IO9 10 kΩ pull-ups | sch |
| Host components | `espressif/esp_hosted` ~2.12 (lock: 2.12.13), `espressif/esp_wifi_remote` ~1.6 (lock: 1.6.5) | code |
| C6 slave firmware | bench unit 2.12.11 (has BT); factory image reports 0.0.0 and has no hosted Bluetooth | doc, code |
| BT host | NimBLE on the P4, controller on the C6 (`CONFIG_BT_CONTROLLER_DISABLED=y`, `CONFIG_ESP_HOSTED_ENABLE_BT_NIMBLE=y`, UART transport off) | code |
| NimBLE | central + observer only, 1 connection, preferred MTU 260, bonds in NVS, memory in PSRAM, host task stack 8192, log level WARNING | code |
| Mempool | ESP-Hosted transport mempool in PSRAM | code |
| Antenna | internal 3D antenna selected (0x43 P0 low); MMCX external path unused | code, vendor |

Bring-up (`bt_controller_start`, called by `ble_midi.rs` before
`nimble_port_init`): sleep 600 ms ("let the C6 boot"),
`esp_hosted_connect_to_slave()`, read and log the slave firmware version
(warn on 0.0.0), `esp_hosted_bt_controller_init()`,
`esp_hosted_bt_controller_enable()`. Then NimBLE scans for `GZUT-MIDI` or
the BLE-MIDI service UUID, connects (~2.6 s after boot on the bench),
exchanges MTU 260, writes `80 80` + plain MIDI packets of at most 244 bytes
without response. The Tab5 build is **not** `ble_relaxed` (that cfg is for
headless boards and the original ESP32): it keeps the piano's 10 ms
interval and has no write gap. ESP-Hosted's boot constructor only prepares
its transport; the C6 is contacted only in BLE mode (doc). Wi-Fi is not
used by Piano Coach.

### 4.10 Sensors, RTC, camera, other

* BMI270 IMU (0x68), RX8130CE RTC (0x32, backup on a supercapacitor per
  vendor), INA226 (0x41): on the system I²C bus, not used. BMI270 INT1 and
  the RTC interrupt feed the E_TRG wake path of the power controller (sch,
  vendor "Interrupt Wakeup (PMS150G-U06)").
* Camera (MIPI-CSI, MCLK GPIO36, reset 0x43 P6): not used. The BSP's
  `bsp_camera_start` exists but is never called.
* RS-485, Grove, M5-Bus, STAMP: not used.

## 5. Memory map and partitions

Partition table: `firmware/piano-coach/partitions/partitions.csv` (the
Tab5 profile has no `boards/tab5/partitions.csv`; `.cargo/config.toml`
sets `ESP_IDF_GLOB_PARTTABLE_BASE = partitions`, which `tools/board.sh`
leaves alone for this board):

| Name | Type | SubType | Offset | Size |
|---|---|---|---|---|
| nvs | data | nvs | 0x9000 | 24K |
| phy_init | data | phy | 0xf000 | 4K |
| factory | app | factory | 0x10000 | 12M |

The file's comment: "The embedded CJK glyph set makes the app large; songs
live on the microSD card, so no data partition is needed." The app image
was 7.6 MB of the 12 MB partition after the speaker/globe commit
(`doc/piano_coach_firmware.md`; an older line there still says ≈ 4.9 MB).
About 3.9 MB of flash after the app partition is unallocated. NVS holds the
`piano` namespace (prefs, i32) and NimBLE bonds.

Notable `boards/tab5/sdkconfig.defaults` settings, with their comments:

| Setting | Why (quoted) |
|---|---|
| `CONFIG_ESP32P4_SELECTS_REV_LESS_V3=y`, `CONFIG_ESP32P4_REV_MIN_100=y` | "Tab5 silicon is rev v1.3; IDF 5.5 otherwise links a rev-3 bootloader that faults on its first instruction." |
| `CONFIG_ESPTOOLPY_FLASHSIZE_16MB=y`, custom `partitions.csv` | — |
| `CONFIG_FREERTOS_HZ=1000` | "1 kHz scheduler tick: MIDI timing needs ms resolution (100 Hz would quantise every sleep to 10 ms)." |
| `CONFIG_ESP_MAIN_TASK_STACK_SIZE=6144`, `CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT=16384`, `CONFIG_ESP_TASK_WDT_TIMEOUT_S=15` | "Keep the IDF main task bootstrap-only (its stack comes from the small pre-scheduler heap); main.rs spawns the real application task." |
| `CONFIG_SPIRAM=y`, `SPIRAM_SPEED_200M`, `SPIRAM_USE_MALLOC`, `IDF_EXPERIMENTAL_FEATURES`, `SPIRAM_XIP_FROM_PSRAM=y`, `SPI_FLASH_AUTO_SUSPEND=n`, `SPIRAM_MALLOC_ALWAYSINTERNAL=4096`, `SPIRAM_MALLOC_RESERVE_INTERNAL=32768` | "32 MB PSRAM at 200 MHz for framebuffers, the render shadow buffer, songs." |
| `CONFIG_BSP_LCD_DPI_BUFFER_NUMS=2` | "Two DPI framebuffers: render into the back buffer, then swap." |
| `CONFIG_FATFS_LFN_HEAP=y`, `FATFS_MAX_LFN=255`, `FATFS_API_ENCODING_UTF_8=y`, `FATFS_PER_FILE_CACHE=n`, `FATFS_USE_LABEL=y`, `VFS_MAX_COUNT=8` | "microSD: long file names in UTF-8 (the song library is Chinese)." |
| `CONFIG_ESP_HOSTED_PRIV_SDIO_PIN_*_SLOT_1`, `ESP_HOSTED_SDIO_GPIO_RESET_SLAVE=15`, `ESP_HOSTED_SDIO_CLOCK_FREQ_KHZ=25000` | "ESP-Hosted SDIO link: the Tab5 wires the C6 to SDIO slot 1 as below." |
| `CONFIG_ESP_HOSTED_MEMPOOL_PREFER_SPIRAM=y` | "Transport mempool in PSRAM (internal RAM would be exhausted at init)." |
| `CONFIG_BT_ENABLED`, `BT_NIMBLE_ENABLED`, `BT_CONTROLLER_DISABLED`, `BT_NIMBLE_TRANSPORT_UART=n`, `ESP_HOSTED_ENABLE_BT_NIMBLE` | "NimBLE host on the P4, controller on the C6 over hosted HCI." |
| `BT_NIMBLE_MEM_ALLOC_MODE_EXTERNAL`, `HOST_TASK_STACK_SIZE=8192`, roles central+observer only, `MAX_CONNECTIONS=1`, `ATT_PREFERRED_MTU=260`, `NVS_PERSIST` | (Bluetooth block: "Settings from the herdline Tab5 client. Only one transport runs per boot: in USB mode the C6 stays unpowered and none of this starts.") |
| `CONFIG_BT_NIMBLE_LOG_LEVEL_WARNING=y` | "NimBLE logs every GATT write at INFO … two console lines per MIDI packet during playback." |

From the generated sdkconfig (defaults, not set by the profile): console
primary UART0 at 115200 plus secondary USB Serial/JTAG
(`CONFIG_ESP_CONSOLE_SECONDARY_USB_SERIAL_JTAG=y`); flash DIO 80 MHz; CPU
360 MHz; `CONFIG_BSP_I2C_NUM=1`, `CONFIG_BSP_I2S_NUM=1`,
`CONFIG_BSP_DISPLAY_BRIGHTNESS_LEDC_CH=1`.

Other memory facts (code): the `app` thread has a 24 KB stack, the UI
thread 96 KB; Slint renders into a PSRAM shadow buffer and copies dirty
rows into the back DPI framebuffer. Release profile: `opt-level = 3` for
piano-core, the firmware and Slint's software renderer/core, `"s"`
elsewhere, thin LTO.

## 6. Build, flash, console

```bash
cd firmware/piano-coach
tools/board.sh tab5 build --release
tools/board.sh tab5 run --release -- --port /dev/ttyACM0   # flash + monitor (blocks)
```

* `tools/board.sh tab5` sources `boards/tab5/board.env` (`FEATURE=board-tab5`,
  `MCU=esp32p4`, `ESP_IDF_VERSION=v5.5.3`, `FLASHER=espflash`, `UI=full`,
  `HAS="usb sd speaker"`), sets
  `ESP_IDF_TOOLS_INSTALL_DIR=custom:/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`
  (override with `workspace` to let embuild download), the sdkconfig
  defaults, `ESP_IDF_SDKCONFIG=sdkconfig.tab5`, the extra-components file,
  and `CARGO_TARGET_DIR=target/tab5`, then runs
  `cargo <cmd> --target riscv32imafc-esp-espidf --no-default-features --features board-tab5`.
* A bare `cargo build` in `firmware/piano-coach` also builds the Tab5
  (defaults in `.cargo/config.toml`), but into `target/`, not `target/tab5`.
* First build compiles ESP-IDF (10+ min); run it in the background.
* Needs `cargo install ldproxy espflash`.
* `run` uses `flash.sh` as the cargo runner: `espflash flash --monitor`
  with **this build's** `bootloader.bin` and `partition-table.bin`.
  `.cargo/config.toml`: "espflash's bundled P4 bootloader has a different
  memory map and does not boot on rev 1.x silicon."
* Direct flash (as used in sessions):
  `espflash flash --chip esp32p4 --port P --baud 921600 --bootloader T/bootloader.bin --partition-table T/partition-table.bin T/piano-coach`
  with `T=target/tab5/riscv32imafc-esp-espidf/release` (`flash.sh` also
  looks in `T/build/`).
* espflash sometimes fails once with "Error while connecting" — retry
  (AGENTS.md).
* Do not use `espflash monitor --before no-reset`: it leaves the chip in
  the ROM bootloader (firmware doc).
* Port identification: the USB-C port enumerates as Espressif USB
  Serial/JTAG `303a:1001` on `/dev/ttyACM*`; check with
  `udevadm info -q property -n /dev/ttyACMx | grep ID_SERIAL` (contains the
  MAC `30:ed:a0:e2:fa:9b` on the bench unit) or
  `espflash board-info --port …`. Port numbers change on re-plug.
* Console: logs go to UART0 (GPIO37/38, on M5-Bus) and to the USB
  Serial/JTAG port. For logging from a script, open the port with pyserial
  and DTR/RTS false; an RTS pulse resets the board (AGENTS.md).
* Download mode by hand (vendor): with USB or battery power, hold the
  reset button ~2 s until the internal green LED flashes rapidly, then
  release. Normally not needed: espflash resets into the bootloader over
  USB Serial/JTAG.
* Setting a pref from the PC: build an NVS image with
  `nvs_partition_gen.py generate prefs.csv nvs.bin 0x6000` (namespace
  `piano`, i32 values) and `espflash write-bin --port P 0x9000 nvs.bin`.
  This replaces all saved prefs and BLE bonds (AGENTS.md). Example: key
  `transport` = 1 forces BLE mode.

## 7. Piano Coach support

* **UI class**: `full` — Library (folders, bookmarks, recent, song globe),
  Search (pinyin), Play (falling notes or sheet music, A–B loop), Settings.
  1280×720 logical canvas at scale 1.0, rotated 270°.
* **Capabilities** (`CAPS`): name "M5Stack Tab5", backlight yes, power latch
  yes, USB-MIDI yes, BLE-MIDI yes, buttons no, touch yes.
* **Transports**: USB-A host **or** BLE via the C6, one per boot. NVS key
  `transport`: absent/0 = USB (the default), 1 = BLE. Settings → Connect
  via saves it, shows "Switching to … — restarting…" and calls
  `esp_restart()` after 900 ms.
* **Speaker**: Settings → Notes sound from: Piano (default) / Speaker /
  Both (NVS `sound`), speaker volume (`spkvol`).
* **Boot order** (`main.rs`): `early_init` (I²C bus recovery, expander
  handles) → NVS prefs → transport choice → `init_power(transport)` →
  `mount_sd` → transport start → audio thread → player → UI thread
  (`display_init`).

Verified on hardware (bench unit: P4 rev v1.3, ST7123, C6 2.12.11):

| Date | Check | Result |
|---|---|---|
| 2026-10-15 | Boot, display, touch, dark UI | ✔ |
| 2026-10-15 | USB: piano enumerates (`CHMidi-2.3`, 5022:0716, IN 0x82 / OUT 0x02); all 88 LEDs on/off | ✔ |
| 2026-10-15 | microSD mount, Chinese names render | ✔ |
| 2026-10-15 | BLE: C6 controller up, NimBLE sync, connect ~2.6 s after boot, handle 9, MTU 260; 88 LEDs; Learn | ✔ |
| 2026-10-15 | USB Learn / Follow / Listen end to end | reported working via the UI; not separately logged |
| 2026-10-15 | Search, A–B loop (over BLE), bookmarks and scores, song globe, sheet music | ✔ |
| 2026-10-15 | Speaker playback (no stutter on a dense song, 18–32 % of a core) | ✔ |
| 2026-10-15 | Shut down with USB-C connected: LEDs off, latch pulses, deep-sleep fallback, power button boots | ✔ |
| 2026-10-16 | Piano-key remote over BLE (all control keys), shuffle-play, boot into Follow with a 儿时回忆 song | ✔ |
| 2026-10-16 | F1/E1 by category on microSD; mid-song control-key window (山丹丹开花红艳艳) | ✔ |

Not verified on the Tab5:

* Shut-down on battery only (the latch actually cutting power).
* The 500 ms stale-key drop on USB attach (`STALE_MS`, verified on the
  YD-ESP32-S3 only).
* v1 hardware (ILI9881C + GT911) and ST7121 panels.
* Headphone output; INA226/RTC/IMU (unused).

Known limits:

* No simultaneous USB and BLE (see §8).
* A C6 with factory firmware (0.0.0) cannot do BLE: the connection status
  reads "Bluetooth error — C6 Bluetooth unavailable (update its
  ESP-Hosted firmware): …" (and the log warns "factory C6 firmware 0.0.0
  has no hosted Bluetooth"). The repo has no
  C6 updater; herdline's `c6ota` feature (external project) does it over
  Wi-Fi.
* Only one BLE central can hold the piano: a PC or tablet connected to
  `GZUT-MIDI` blocks the Tab5, and vice versa.
* Globe ~10 fps while spinning (PSRAM bandwidth shared with scanout).

## 8. Quirks and lessons learned

* **USB-HS host and C6 SDIO interfere** (found in herdline): only one
  transport per boot. USB mode powers USB-A 5 V and leaves the C6 unpowered;
  BLE mode powers the C6 and leaves USB-A off. Switching restarts.
* **Wedged system I²C bus**: `early_init` bit-bangs SCL (GPIO32) up to 16
  clocks while SDA (GPIO31) is held low, then sends a STOP, before any
  driver is created.
* **Create expander handles once**: "the driver's one-time reset clears
  every output, and the expanders keep their state across reboots".
  `early_init` retries `bsp_io_expander_init`/`…1_init` up to 12 times,
  250 ms apart. After that, `init_power` programs both expanders by raw
  register writes (values from M5Tab5-UserDemo).
* **Expanders are reset by SOC_RST** (sch): a hardware reset of the P4
  also resets them; `esp_restart()` does not.
* **LCD "power cycle" is a reset pulse**: `BSP_FEATURE_LCD` toggles 0x43 P4,
  which the schematic and vendor call LCD_RST; the panel's 3.3 V comes
  from SOC_3.3V with no switch. The 400 ms low / 250 ms settle is what
  makes warm reboots re-train the DSI link.
* **Vendor reset caveat (not applied)**: for ST7123/ST7121, vendor says
  LCD_RST "must be configured as push-pull or open-drain while it is driven
  low, and switched to input pull-up while it is driven high; otherwise,
  the stability of I2C bus communication will be affected". The BSP and
  firmware drive it push-pull both ways and no I²C problem has been
  logged — keep this in mind if the bus misbehaves.
* **ST7123 timing patch**: 120 ms after SLPOUT, 50 ms after DISPON (else
  random washed-out boots).
* **Panel probe asserts** if neither 0x55 nor 0x14 answers ("Unsupported
  board version!").
* **Codec waits for the display**: the ES8388 is on the I²C bus created
  during display bring-up; `speaker_codec()` spins on `DISPLAY_UP`.
* **Amp open only while sounding**: an idle open NS4150B hisses.
* **Cache sync alignment**: `esp_cache_msync` on the DPI framebuffer needs
  64-byte alignment → bands rounded to even rows.
* **P4 rev 1.x bootloader**: needs `CONFIG_ESP32P4_SELECTS_REV_LESS_V3`
  and flashing with the build's own bootloader, never espflash's bundled
  one.
* **IDF version**: `.cargo/config.toml`: "v5.5: v5.4.x can place a task
  stack in the P4's TCM during startup (espressif/esp-idf#15996)."
* **Main task**: its stack comes from the small pre-scheduler heap, so
  `main()` only spawns the `app` thread (24 KB).
* **ESP-Hosted mempool in PSRAM**, else internal RAM runs out at init.
* **USB host BSP patches** (herdline, in `bsp_usb.c`): install failure
  returns an error and drops the 5 V rail instead of aborting;
  `usb_host_device_free_all` errors are logged, not fatal; `bsp_usb_host_stop`
  deletes the event task before uninstalling.
* **NimBLE INFO logging** of every write floods the console — log level is
  WARNING.
* **Charging needs the firmware**: the IP2326 charges only with CHG_EN
  high, which `init_power` sets in both modes.
* **Piano queues key presses** on USB while nothing reads them and delivers
  them at attach; the first 500 ms of key events are dropped.

Discrepancies between sources:

| Topic | Code / BSP | Schematic / vendor | Which the firmware uses |
|---|---|---|---|
| 0x44 P6 | `mod.rs` comment: "P6 USB-C detect (in)" | sch: CHG_STAT (IP2326 LED pin); vendor: CHG_STAT_LED | configured as input, never read |
| 0x44 P5 | comment: "QC_CHG_EN (active low)" | sch: nCHG_QC_EN switches an NTC resistor via Q1 | driven low |
| 0x43 P4 | BSP: `BSP_LCD_EN`; `mod.rs`: "power-cycle the LCD rail" | sch, vendor: LCD_RST | toggled as in code |
| USB pins | BSP header: `BSP_USB_POS = GPIO20`, `BSP_USB_NEG = GPIO19` (unused by BSP code) | sch: GPIO20 = RS485_TX, GPIO19 = xSPI_MISO; USB-A is on the dedicated HS OTG pins | HS OTG via the IDF host library; the macros are ignored |
| Camera | BSP: sensor SC202CS, `BSP_CAMERA_GPIO_XCLK = NC` | vendor: SC2356, CAM_MCLK = G36 | camera unused |
| Touch controller | BSP README says GT911 | vendor: ST7123 since 2025-10-14, ST7121 since 2026-04-28 | BSP probes 0x55 then 0x14 |
| ST7121 | no driver branch in the BSP | vendor: current production panel | unknown whether the ST7123 path works |
| PSRAM mode | generated sdkconfig: hex mode | vendor: "32MB Octal" | hex (IDF default for P4) |
| USB-C role | console/JTAG | vendor: "USB Type-C (USB 2.0 OTG)"; sch: FS USB Serial/JTAG pins, 5.1 kΩ CC pull-downs | console/flash |
| BLE packet size | `blemidi::MAX_PACKET = 244` | `GZUT_piano_protocol.md`: "up to 257 bytes (MTU 260)" verified earlier | 244 |
| App size | firmware doc: "Image ≈ 7.6 MB of the 12 MB partition" | same doc, older line: "image ≈ 4.9 MB" | — (7.6 MB is newer) |
| Architecture note | firmware doc names `board.rs` | code: `src/board/tab5/mod.rs` | — |

Open questions:

* Does an ST7121 panel work with the ST7123 init table and touch driver?
* Does the battery-only shut-down cut power as intended?
* What exactly U28 (PMS150G-U06) does on a long press / double press, and
  whether PWROFF_PLUSE is honoured while USB-C is powering the board (the
  bench test fell through to deep sleep).
* What the on-chip LDO channel 4 that the BSP claims for the SD card
  actually feeds (the card's VDD is SOC_3.3V on the schematic).
* The camera's SCCB address and the INA226/RX8130CE addresses were not
  checked against their address pins (vendor values only).

## 9. Sources

In this repo:

* `AGENTS.md`
* `README.md`
* `doc/piano_coach_firmware.md` (board table, shut-down, build, architecture,
  "Verified on hardware (2026-10-15)", piano-key remote results, known
  limits)
* `doc/GZUT_piano_protocol.md` (USB pacing, ESP32-P4 NimBLE note)
* `doc/M5Stack-Tab5/tab5.md` (vendor product page: specs, pin maps,
  expander tables, power/charging/download notes, version history)
* `doc/M5Stack-Tab5/Tab5.pdf` (same page as PDF; pages 8–11 embed the block
  diagram and the five schematic sheets used for "sch")
* `doc/M5Stack-Tab5/esp32-p4_datasheet_en.pdf` (P4NRW32 variant)
* `firmware/piano-coach/boards/tab5/board.env`,
  `boards/tab5/sdkconfig.defaults`, `boards/tab5/esp-idf-components.toml`
* `firmware/piano-coach/boards/README.md`
* `firmware/piano-coach/partitions/partitions.csv`
* `firmware/piano-coach/.cargo/config.toml`, `Cargo.toml`,
  `rust-toolchain.toml`, `build.rs`, `flash.sh`, `tools/board.sh`,
  `components_esp32p4.lock`
* `firmware/piano-coach/src/board/tab5/mod.rs`, `src/board/mod.rs`,
  `src/board/contract.rs`, `src/bindings.h`
* `firmware/piano-coach/src/main.rs`, `audio.rs`, `usb_midi.rs`,
  `ble_midi.rs`, `link.rs`, `prefs.rs`, `threads.rs`, `ui.rs` (transport
  switch, shut-down, brightness), `browse.rs` (library root)
* `firmware/piano-core/src/blemidi.rs` (`MAX_PACKET`)
* `firmware/piano-coach/components/m5stack_tab5_noglib/`: `include/bsp/*.h`,
  `src/*.c`, `priv_include/disp_init_data*.h`, `Kconfig`,
  `idf_component.yml`, `README.md`, `m5stack_tab5.json`
* Generated build files (not in git):
  `target/tab5/riscv32imafc-esp-espidf/release/build/esp-idf-sys/*/out/sdkconfig`
  and `…/out/managed_components/` (I²C address macros of the touch,
  expander, codec and IMU drivers)
* Git history: commits 6320820, 7a0fa03, e32d33a, a91b262, 71d2954,
  7894ab8, 7071ff1, c82f7ce

Not in this repo (referenced by code comments; not read for this document):
M5Tab5-UserDemo (expander register values, `bsp_generate_poweroff_signal`),
the herdline Tab5 client (`~/w/rust/herdline/clients/tab5`), and the C6
updater in herdline (`c6ota`).
