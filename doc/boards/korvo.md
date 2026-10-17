# ESP32-S31-Korvo-1 V1.1 (`korvo`)

Hardware reference for developing Piano Coach on Espressif's
ESP32-S31-Korvo-1. Facts the firmware drives come from the code and are
known to work; facts taken only from the schematic, the Espressif BSP or a
datasheet are labelled as such. Where sources disagree, the conflict is
spelled out together with what the firmware does.

Vendor files copied into the repo:
[vendor/korvo/esp32-s31-korvo-1-schematics.pdf](vendor/korvo/esp32-s31-korvo-1-schematics.pdf)
(4 sheets: 01 Overview, 02 Power, 03 S31 & peripherals, 04 Audio; rev 1.1,
May 2026) and
[vendor/korvo/esp32-s31-korvo-1-dimensions.pdf](vendor/korvo/esp32-s31-korvo-1-dimensions.pdf).

## 1. Summary

The ESP32-S31-Korvo-1 is Espressif's multimedia development board for the
ESP32-S31 (dual-core RISC-V, Wi-Fi 6, Bluetooth 5.4 LE + Classic,
802.15.4). It carries an ESP32-S31-WROOM-3 module (16 MB flash, 16 MB
PSRAM), an ES8389 codec driving two NS4150 speaker amplifiers, two analog
microphones, a microSD slot (4-bit SDIO), a USB 2.0 high-speed Type-A host
port, a DVP camera connector, a WS2812 RGB LED, a four-key ADC button
ladder, BOOT/RESET keys, a CP2102N USB-UART console and a 2×20 header for
an LCD sub-board. The bench unit uses the 4.3″ 800×480 parallel-RGB
sub-board (same panel as the ESP32-S3-LCD-EV-Board SUB3) with a
capacitive touch controller. For Piano Coach it is a **full-UI** board:
the 1280-wide UI drawn at 0.625 scale, songs on microSD, the piano over
USB-A or native BLE, and the speaker synth.

| Item | Value | Source |
|---|---|---|
| Vendor / product | Espressif ESP32-S31-Korvo-1, revision **V1.1** | schematic title block, BSP |
| Board dir / Cargo feature | `korvo` / `board-korvo` | `boards/korvo/board.env`, `Cargo.toml` |
| Module | ESP32-S31-WROOM-3 (PCB antenna; the -3U variant has an external antenna connector) | schematic sheet 3, WROOM-3 datasheet |
| CPU | ESP32-S31, 32-bit RISC-V dual core up to 320 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ=320` in the generated sdkconfig), ULP RISC-V; 512 KB HP SRAM, 32 KB LP SRAM, 320 KB ROM | S31 datasheet |
| Rust target / toolchain | `riscv32imafc-esp-espidf`, the repo's nightly (`rust-toolchain.toml`), `build-std` | `board.env` (default TARGET), `.cargo/config.toml` |
| ESP-IDF | an ESP-IDF **master** checkout (≥ 6.1) at `/home/nws/w/esp32/esp-idf` (`IDF_PATH` overrides); picolibc | `board.env` |
| Flash | 16 MB, QIO, 80 MHz | `sdkconfig.defaults`, generated sdkconfig |
| PSRAM | 16 MB, octal (`CONFIG_SPIRAM_MODE_OCT=y`), 250 MHz, PSRAM-XIP | `sdkconfig.defaults`, generated sdkconfig |
| USB-UART | Silicon Labs **CP2102N** (U15) on the USB-C port labelled UART (J1); VID `10c4` (PID not recorded in the repo); appears as `/dev/ttyUSB*` (last seen `/dev/ttyUSB0`) | schematic sheet 2, AGENTS.md |
| Board size | 106.50 mm × 89.20 mm overall (see §4.11) | dimensions drawing |
| Bench unit | MAC address and chip revision: unknown (not recorded). ESP-IDF used for the 2026-10-15 verification: master v6.1-dev (4d59230ddff) | doc/piano_coach_firmware.md |

## 2. Feature overview

| Component | Part | Function | Bus | Address / pins | Used by Piano Coach |
|---|---|---|---|---|---|
| Module | ESP32-S31-WROOM-3 (U10) | MCU, 16 MB flash, 16 MB PSRAM, radio | — | — | yes |
| LCD sub-board | 4.3″ 800×480 TFT, ST7262E43 per korvo-radio (BSP comment: "SUB3 ST7262 panel") | display | 16-bit parallel RGB (DPI) | J14 2×20 header; data GPIO8–19, 33–36; PCLK 40, DE 43, HSYNC 44, VSYNC 45 | yes |
| Touch (on the sub-board) | GT1151-compatible; korvo-radio's unit reports product ID "1158" | capacitive touch | I²C (shared) | 0x14 (7-bit); no INT/RST wired | yes (polled) |
| Audio codec | Everest **ES8389** (U5) | DAC to speakers, ADC from mics | I²C control + I²S0 | I²C 0x10 (7-bit) = 0x20 (8-bit, as on schematic/driver); I²S GPIO2–6 | yes (DAC only) |
| Speaker amps | 2 × NS4150 (U9 left, U8 right), 3 W class-D | speakers J13 (L), J15 (R), PH2.0 2-pin | analog | enable PA_CTRL = GPIO7 | yes (via codec driver) |
| Microphones | 2 × LMA3729T381-OY3S analog MEMS (U6, U7) | mic array → ES8389 MIC1/MIC2 | analog | — | no |
| microSD | TF slot J7, P-FET power switch Q1 (AO3401A) | song library | SDMMC 4-bit (slot 0) | D0–D3 GPIO20–23, CLK 24, CMD 25, power GPIO39 (active low) | yes |
| SPI NAND (footprint) | U4, with ME6211C18 1.8 V LDO (U1) | stuffing option instead of the SD slot | SPI on GPIO20–25 | U4, its supply resistors R134/R135 and series resistors R22/R23/R1–R4 are all marked NC | no (not fitted per schematic) |
| USB-A host | J2 (USB-02), TPS2051CDBVR (U14) 500 mA switch, ESD7951ST5G/LESD5D5.0CT1G ESD | piano over USB-MIDI | USB 2.0 HS (dedicated USB_DP/USB_DM pads, UTMI PHY) | no GPIO | yes |
| USB-UART | CP2102N (U15) on USB-C J1, auto-program transistor pair U16 (BC847BDW1T1G) | console, flashing | UART0 | TX0 = GPIO58, RX0 = GPIO59; DTR/RTS → EN/GPIO61 | yes (console) |
| Power-in USB-C | J5 (labelled POWER) | 5 V input only (data pins not connected) | — | — | — |
| Power switch | SS-12D11-G040 slide switch (J3, labelled ON) | VCC_IN → VCC_5V | — | — | — |
| 3.3 V regulator | TLV62569 buck (U11), 2.2 µH | VCC_3V3 for module and logic | — | — | — |
| Audio 3.3 V | ME6211C33M5G-N LDO (U2) | Aud_3V3 for ES8389 and mics | — | — | — |
| Camera rails | ME6211C28 (U12, DOVDD 2V8), ME6211C15 (U13, DVDD 1V5) | camera supply | — | — | no |
| Camera connector | J4 FPC-24P | DVP camera (BSP: OV3660; korvo-radio's unit: SC101IOT) | DVP 8-bit + SCCB on shared I²C | GPIO46–57; SCCB 0x78 (8-bit, OV3660 per schematic) / 0x68 (7-bit, SC101IOT per korvo-radio) | no (driver linked, not called) |
| ADC keys | SW3 SET, SW4 MODE (silk "MODE", Piano Coach "PLAY"), SW5 VOL−, SW6 VOL+ | four-key resistor ladder | ADC1 | GPIO42 (ADC1_CH0_N) | yes |
| BOOT / RESET keys | SW2 (GPIO61 to GND), SW1 (EN to GND) | download mode / reset | — | GPIO61, EN | RESET wakes from shut-down |
| RGB LED | WS2812B (U3), 5 V, data level-shifted by LBSS138LT1G (Q2) | status LED | RMT | GPIO37 | no |
| Power LED | red LED D18 on VCC_5V | power indicator | — | — | — |

## 3. Pin map

Columns: direction / active level as the hardware uses it; "FW" = used by
Piano Coach firmware (via the BSP in `components/esp32_s31_korvo`).
"BSP" means `components/esp32_s31_korvo/include/bsp/esp32_s31_korvo.h`
(the fork the firmware builds); "sch" means the schematic.

| GPIO | Net (schematic) | Function | Dir / level | Peripheral | FW | Source |
|---|---|---|---|---|---|---|
| 0 | GPIO0 = LCD_I2C_SDA = CAM_I2C_SDA | I²C SDA (touch, codec CDATA via 33 Ω R25, camera SCCB) | bidir, 4.7 kΩ pull-up R18 to VCC_3V3 | I2C_NUM_0 master, internal pull-up also enabled | yes | BSP, sch |
| 1 | GPIO1 = LCD_I2C_SCL = CAM_I2C_SCL | I²C SCL (codec CCLK via 33 Ω R26) | out, 4.7 kΩ pull-up R19 | I2C_NUM_0 | yes | BSP, sch |
| 2 | I2S_MCLK (R95 0 Ω) | I²S MCLK to ES8389 pin 8 | out | I2S0 | routed by the BSP, but the codec is configured `use_mclk = false` (see §4.6) | BSP, sch |
| 3 | I2S_SCLK (R96) | I²S BCLK | out (SoC is master) | I2S0 | yes | BSP, sch |
| 4 | I2S_LRCK (R97) | I²S WS | out | I2S0 | yes | BSP, sch |
| 5 | I2S_DSDIN (R98) | I²S data SoC → codec DSDIN (playback) | out | I2S0 TX (`dout`) | yes | BSP (`BSP_AUDIO_I2S_SDOUT`), sch |
| 6 | I2S_SDOUT | I²S data codec ASDOUT → SoC (microphones) | in | I2S0 RX (`din`) | channel enabled, never read | BSP (`BSP_AUDIO_I2S_DSIN`), sch |
| 7 | PA_CTRL (R99) | enable both NS4150 amps | out, active high; 10 kΩ pull-downs R83/R87 keep the amps off at reset | GPIO via esp_codec_dev | yes | BSP, sch |
| 8–12 | LCD_B3…B7 | RGB data D0–D4 | out | LCD RGB | yes | BSP, sch |
| 13–18 | LCD_G2…G7 | RGB data D5–D10 | out | LCD RGB | yes | BSP, sch |
| 19 | LCD_R3 | RGB data D11 | out | LCD RGB | yes | BSP, sch |
| 20 | SD_D0 | microSD DAT0 (NAND footprint shares it) | bidir | SDMMC slot 0 | yes | BSP, sch |
| 21 | SD_D1 | microSD DAT1 | bidir | SDMMC | yes | BSP, sch |
| 22 | SD_D2 | microSD DAT2 | bidir | SDMMC | yes | BSP, sch |
| 23 | SD_D3 | microSD DAT3/CD | bidir | SDMMC | yes | BSP, sch |
| 24 | SD_CLK | microSD CLK | out | SDMMC | yes | BSP, sch |
| 25 | SD_CMD | microSD CMD | bidir | SDMMC | yes | BSP, sch |
| 33–36 | LCD_R4…R7 | RGB data D12–D15 | out | LCD RGB | yes | BSP, sch |
| 37 | WS2812_CTRL (R21) | WS2812B data (via BSS138 level shifter to 5 V) | out | RMT 10 MHz | no (BSP has `bsp_led_*`, not called) | BSP, sch |
| 38 | LCD_CS (R139); GM_FK via R46 (NC) | sub-board SPI CS (panel init interface, unused by this panel); camera GM_FK if R46 fitted | out | — | no | BSP, sch |
| 39 | SD_CTRL (R146) | microSD power: gate of P-FET Q1 | out, **active low** (low = card powered); 10 kΩ pull-down R138 powers the card by default | GPIO | yes (driven low before mount) | BSP, sch |
| 40 | LCD_PCLK | RGB pixel clock 18 MHz | out, data latched on falling edge | LCD RGB | yes | BSP, sch |
| 42 | BT_ARRAY_ADC (R78) | ADC key ladder | analog in | ADC1 channel 0, **negative input** (ADC1_CH0_N), 0 dB attenuation | yes | BSP, sch, WROOM datasheet |
| 43 | LCD_H_EN | RGB DE | out | LCD RGB | yes | BSP, sch |
| 44 | LCD_H_SYNC | HSYNC | out | LCD RGB | yes | BSP, sch |
| 45 | LCD_V_SYNC | VSYNC | out | LCD RGB | yes | BSP, sch |
| 46–53 | DVP_Y2…Y9 | camera D0–D7 | in | DVP | no | BSP, sch |
| 54 | DVP_PCLK | camera PCLK (also JTAG MTDO) | in | DVP | no | BSP, sch |
| 55 | XMCLK | camera XCLK 20 MHz (also MTCK) | out | DVP | no | BSP, sch |
| 56 | DVP_VSYNC | camera VSYNC (also MTDI) | in | DVP | no | BSP, sch |
| 57 | DVP_HREF | camera HREF (BSP `BSP_CAMERA_HSYNC`, also MTMS) | in | DVP | no | BSP, sch |
| 58 | TX0 | UART0 TXD → CP2102N RXD (via R122 0 Ω) | out | UART0 console 115200 | yes | sch, WROOM datasheet |
| 59 | RX0 | UART0 RXD ← CP2102N TXD (via R121 470 Ω) | in | UART0 | yes | sch, WROOM datasheet |
| 60 | LCD_MOSI (R141) | sub-board SPI MOSI; strapping pin; 10 kΩ pull-up R101 not fitted | out | — | no | BSP, sch |
| 61 | LCD_SCK (R140) = BOOT_IO61 | sub-board SPI SCK **and** BOOT key SW2 (to GND) and auto-program; strapping pin; 10 kΩ pull-up R102 not fitted | in at reset | — | no | BSP, sch |
| EN | EN_RST | chip enable; RESET key SW1 to GND, RC 10 kΩ (R17) / 1 µF (C15), auto-program | in | — | — | sch |
| USB_DP / USB_DM (module pins 40/41) | USB_DP_A / USB_DM_A (R9/R8 0 Ω) | USB-A host | — | USB 2.0 HS OTG (UTMI PHY) | yes | sch, WROOM datasheet |

Signals that are **not wired to any GPIO** (BSP `GPIO_NUM_NC`): LCD
backlight, LCD DISP_EN, LCD reset, touch INT, touch RST, camera PWDN
(pulled down by R63 10 kΩ = normal), camera RESET (pulled up by R61 10 kΩ =
normal), SD card-detect (`SD_DET` goes only to test point TP5; the BSP uses
`SDMMC_SLOT_NO_CD`).

GPIOs not listed (26–32, 41) do not exist or are not bonded on the WROOM-3
(module pins 33–39 are NC).

### I²C bus (I2C_NUM_0, SDA GPIO0, SCL GPIO1)

| 7-bit addr | Device | Notes |
|---|---|---|
| 0x10 | ES8389 codec | schematic and `ES8389_CODEC_DEFAULT_ADDR` say **0x20** — the 8-bit write form; esp_codec_dev's `audio_codec_ctrl_i2c.c` passes `addr >> 1` to the I²C driver. AD0/AD1 straps pulled to GND (R30/R28 100 kΩ). The firmware passes 0x20 to esp_codec_dev, which is correct for that API |
| 0x14 | Touch controller (GT1151 driver) | `ESP_LCD_TOUCH_IO_I2C_GT1151_ADDRESS`; schematic sheet 3 annotates "I2C ADDRESS: 0x14" at J14. korvo-radio: 0x5D also valid; product ID "1158". The BSP runs it at 400 kHz |
| 0x3C (0x78 8-bit) | OV3660 camera, if fitted | schematic annotation "OV3660 I2C ADDRESS: 0x78"; not used |
| 0x68 | SC101IOT camera | found by korvo-radio on its unit once XCLK runs; not used |

### ADC key ladder (GPIO42)

Schematic sheet 4 "KEY Array": BT_ARRAY_ADC is pulled up to VCC_3V3 by
R77 10 kΩ; each key pulls it to GND through its own resistor. The ADC
range is 0–2 V (0 dB attenuation), so the idle level (3.3 V at the pin)
reads as the top of the range; the BSP treats idle as 2000 mV.

| Key (silk) | Resistor | Level (schematic) | BSP enum | BSP window (mV, midpoints) | Piano Coach action |
|---|---|---|---|---|---|
| VOL+ | R82 1.3 kΩ | 0.38 V | `BSP_BUTTON_VOLUP` = 0 | 160–600 | speaker volume +10 % |
| VOL− | R81 3.3 kΩ | 0.82 V | `BSP_BUTTON_VOLDOWN` = 1 | 600–1080 | speaker volume −10 % |
| MODE | R80 6.8 kΩ | 1.34 V | `BSP_BUTTON_MODE` = 2 | 1080–1605 | **PLAY**: play / pause |
| SET | R79 13 kΩ | 1.87 V | `BSP_BUTTON_SET` = 3 | 1605–1935 | next page (Library → Play → Settings) |

The windows are computed in `bsp_iot_button_create` from the centres
380 / 820 / 1340 / 1870 mV and the idle 2000 mV. Each read averages four
`adc_oneshot` samples and converts raw → mV with the BSP's own S31
software calibration (`esp32_s31_adc_calibration.c`: regi2c writes to the
SAR blocks 0x10/0x11 to enable raw output and `cal_done`, then a weighted
bit-code conversion with `BSP_S31_ADC_MAX_MV` 2000). GPIO42 is the
*negative* input of a differential channel, so the code is inverted: idle
reads raw 0 and a key press raises the code (korvo-radio README). Clicks
are `iot_button` `BUTTON_SINGLE_CLICK` events.

**Conflict:** Espressif's esp-bsp (`esp-bsp/bsp/esp32_s31_korvo_1`,
`bsp_button.c` and the JSON) names 380 mV `VOLM` and 819 mV `VOLP`, i.e.
the opposite of the schematic. The firmware follows the fork BSP, which
agrees with the schematic (0.38 V = VOL+). The keys are not yet pressed on
hardware, so the labels are confirmed only by the schematic.

## 4. Subsystems

### 4.1 Power

From schematic sheet 2:

* Two USB-C receptacles: **J1** (USB to UART, net USB0_5V) and **J5**
  ("Power IN", USB1_5V, data pins unconnected). Each 5 V input goes
  through an AO3401A P-FET with an LMBT3906DW1T1G current-mirror ideal-diode
  controller into **VCC_IN**, so either port powers the board.
* **J3** slide switch (SS-12D11-G040, silk "ON") connects VCC_IN to
  **VCC_5V**. The red LED D18 (2.2 kΩ) shows VCC_5V. korvo-radio notes:
  check this switch before debugging a "disconnected" board.
* **VCC_3V3**: TLV62569 buck (U11) from VCC_5V, 2.2 µH, divider 100 kΩ /
  22.1 kΩ.
* **Aud_3V3**: ME6211C33 LDO (U2) from VCC_5V for the ES8389 and mics.
* **SD_3V3**: VCC_3V3 switched by P-FET Q1 (AO3401A), gate = SD_CTRL
  (GPIO39) with a 10 kΩ pull-down (on unless GPIO39 is driven high).
* **Camera**: DOVDD_2V8 (ME6211C28, U12) and DVDD_1V5 (ME6211C15, U13)
  from VCC_3V3; AVDD_2V8 = DOVDD_2V8 through R45 0 Ω.
* **NAND 1V8**: ME6211C18 (U1) and its parts are marked NC.
* Speaker amps and the WS2812B run from VCC_5V. The LCD header J14
  supplies 3.3 V (pins 35/36) and 5 V (pins 39/40).
* No battery, no fuel gauge, **no power latch**, no backlight control.
  Firmware: `CAPS.power_latch = false`; `power_off()` calls
  `bsp_display_backlight_off()` (a no-op, the pin is NC) and
  `esp_deep_sleep_start()`; the RESET key (or a power cycle) restarts.
  `init_power()` does nothing (USB VBUS is always on, the radio is on-chip).

### 4.2 USB ports

| Port | Connector | Function | Notes |
|---|---|---|---|
| UART (USB-C J1) | CP2102N | console + flashing (UART0, 115200 8N1) | auto-program via DTR/RTS (§6) |
| POWER (USB-C J5) | — | 5 V input only | |
| USB-A HOST (J2) | ESP32-S31 USB 2.0 OTG, HS UTMI PHY | the piano's USB-MIDI | VBUS from TPS2051C |

USB-A host details:

* Data lines go straight to the module's dedicated `USB_DP` / `USB_DM`
  pads (pins 40/41) through 0 Ω R9/R8; no GPIO muxing. The generated
  sdkconfig shows `SOC_USB_UTMI_PHY_NUM=1`, `SOC_USB_FSLS_PHY_NUM=0`.
* VBUS: TPS2051CDBVR (U14), 500 mA limit. Its EN (VBUS_EN) is pulled to
  VCC_5V by R73 10 kΩ (the pull-down R74 is NC), so **VBUS is always on**;
  the FLG output only reaches test point TP17 (overcurrent cannot be read
  by software).
* Firmware: `usb_host_start()` → `bsp_usb_host_start()`
  (`components/esp32_s31_korvo/esp32_s31_usb.c`): a task `s31_usb`
  (4 KB stack, priority 7) **pinned to core 1** installs the host library
  (`ESP_INTR_FLAG_LOWMED`) and services its events. Install, events and
  uninstall stay on the same core because herdline found cross-core
  teardown racing a pending root-port IRQ with hub deletion.
  `CONFIG_USB_HOST_HUBS_SUPPORTED=y`. The MIDI class driver itself is
  `src/usb_midi.rs` (shared with Tab5 and YD-ESP32-S3).
* The S31's USB Serial/JTAG controller cannot be used on this board: its
  pins GPIO33/34 (`USB1P1_N0/P0`) carry LCD R4/R5.

### 4.3 Display

* Sub-board: 4.3″ 800×480, 16-bit RGB565 parallel RGB, plugged into J14
  (2×20 male header). Panel ST7262E43 per korvo-radio
  (`doc/01-board.md`), "a dumb TTL RGB panel — no init sequence".
* J14 pinout (schematic sheet 3): 1, 2, 8, 33, 34, 37, 38 GND; 3, 4, 6, 10
  NC; 5 LCD_I2C_SDA; 7 LCD_I2C_SCL; 9 LCD_H_EN (DE); 11 HSYNC; 12 VSYNC;
  13 B3; 14 PCLK; 15 B5; 16 B4; 17 B7; 18 B6; 19 G3; 20 G2; 21 G5; 22 G4;
  23 G6; 24 G7; 25 R3; 26 R4; 27 R5; 28 R6; 29 R7; 30 LCD_MOSI; 31 LCD_CS;
  32 LCD_SCK; 35, 36 VCC_3V3; 39, 40 VCC_5V.
* Timing used by the firmware (`include/bsp/display.h`, "SUB_BOARD3_800_480_PANEL_35HZ_RGB_TIMING,
  verified on the Korvo demo"):

  | Parameter | Value |
  |---|---|
  | PCLK | 18 MHz, `pclk_active_neg = true` (latch on falling edge) |
  | H: pulse / back porch / front porch | 40 / 40 / 48 |
  | V: pulse / back porch / front porch | 23 / 32 / 13 |
  | Total frame | 928 × 548 → ≈ 35.4 Hz |
  | Format | RGB565, 16-bit bus, `dma_burst_size` 128 |
  | Framebuffers | exactly 2, allocated by the RGB driver in PSRAM (`fb_in_psram`), 768 000 B each |

* **Timing conflict:** the upstream esp-dev-kits BSP
  (`esp-dev-kits/examples/esp32-s31-korvo/.../esp32_s31_korvo/include/bsp/display.h`)
  uses 26 MHz, H 1/40/20, V 1/10/5. The fork's comment says those
  "roll on this panel"; korvo-radio says they do not lock the ST7262E43.
  Espressif's esp-bsp `esp32_s31_korvo_1` already has the 18 MHz values.
  The firmware uses 18 MHz.
* Present path: Slint renders into a shadow buffer; `present()` copies the
  dirty band into the back framebuffer and calls `bsp_display_present(y0,
  y1, fb)`, which calls `esp_lcd_panel_draw_bitmap` (the RGB driver
  switches scanout when handed one of its own framebuffers and does the
  cache writeback) and then waits up to 250 ms for the
  `on_frame_buf_complete` callback: "S31's GDMA link-switch callback
  signals that the old scanout buffer can be reused. VSYNC alone is
  insufficient because DMA prefetch can still hold the old frame."
* Geometry in the firmware (`src/board/korvo/mod.rs`): `FRAME_WIDTH/HEIGHT`
  800×480, `WINDOW_*` 800×480, **`UI_SCALE` 0.625** (the 1280-wide design
  becomes a 1280×768 logical canvas), `RenderingRotation::NoRotation`,
  `fb_index = y*800 + x`. The BSP rejects any rotation other than 0
  ("panel rotation must be applied by the renderer, not the BSP").
* Backlight: `BSP_LCD_BACKLIGHT` = NC; `CAPS.backlight = false`, so the UI
  hides the brightness slider. `set_backlight()` calls the BSP on/off
  functions, which do nothing for an NC pin. The sub-board's SPI lines
  (CS GPIO38, MOSI GPIO60, SCK GPIO61) are routed but unused.
* korvo-radio observed that its own bare-metal DPI driver needed the RGB
  transfer buffer enabled (ESP-IDF's driver does this) to stop drift; with
  ESP-IDF's `esp_lcd` the firmware has no such issue ("looks right").

### 4.4 Touch

* Controller on the LCD sub-board, I²C 0x14 on the shared bus (GPIO0/1),
  **no INT or RST** line (polled). Driver: `espressif/esp_lcd_touch_gt1151`
  at 400 kHz via `bsp_touch_new(BSP_DISPLAY_ROTATE_0, …)`; `x_max` 800,
  `y_max` 480, no swap/mirror.
* Naming conflict: the BSPs and the firmware commit call it **GT1151**;
  `doc/piano_coach_firmware.md` calls it **GT1158**;
  korvo-radio's unit reports product ID "1158" at register 0x8140. The
  GT1151 driver works with it.
* Firmware: `board::poll_touch` reads one point per UI loop; raw
  coordinates map to logical as `x / 0.625, y / 0.625`
  (`touch_to_logical`). Settings → **Show touches** draws a red dot under
  the finger for diagnosis.
* **Dead strip (bench unit):** no points are reported in a full-height
  strip at ≈ 50–62 % of the width (≈ x 400–496 physical). The independent
  korvo-radio demo shows the same strip, so it is the panel/controller,
  not the firmware. The top-bar Settings tab lies in it; the SET key
  cycles pages so every page stays reachable.
* korvo-radio protocol notes (if a custom driver is ever needed): 16-bit
  big-endian register addresses; point data at 0x814E, 8 bytes per point
  with little-endian X/Y; checksum over 1 + 8n + 2 bytes; write 0 to
  0x814E after every read.

### 4.5 Buttons

* Four ADC keys (table in §3). Firmware: `poll_buttons()` lazily calls
  `bsp_iot_button_create`, registers a single-click callback per key that
  sets a bit in an atomic, and returns `VolumeUp`, `VolumeDown`,
  `PlayPause` (BSP "MODE", silk MODE) or `Select` (SET). `ui.rs` maps them:
  PLAY = play/pause, SET = next page, VOL± = speaker volume ±10 %
  (saved as pref `spkvol`, toast shown).
* BOOT (SW2, GPIO61) and RESET (SW1, EN) are plain switches to GND with
  ESD diodes; the firmware does not read BOOT. RESET is the wake-up from
  shut-down (`CAPS.wake_hint = "RESET"`).
* Key order on the board edge (dimensions drawing, left to right): SET,
  MODE, VOL−, VOL+; RESET and BOOT at the top right.

### 4.6 Audio

* **Codec** ES8389 (U5), control on the shared I²C bus (0x10 7-bit), I²S0
  with the **SoC as master** and the codec as slave. Firmware path:
  `speaker_codec()` waits until the display is up (`DISPLAY_UP`, because
  the display bring-up creates the shared I²C bus), then
  `bsp_audio_init(NULL)` (I²S0 TX + RX, Philips, 16-bit stereo, 48 kHz,
  `auto_clear`) and `bsp_audio_codec_speaker_init()` (ES8389 in DAC mode,
  `pa_pin` GPIO7, `use_mclk = false`, `master_mode = false`,
  `pa_voltage` 5.0 V, `codec_dac_voltage` 3.3 V).
* Rates: the synth renders mono at 24 kHz; `audio.rs` upsamples ×2 by
  linear interpolation to `AUDIO_OUT_RATE` 48 kHz and duplicates it to
  `AUDIO_CHANNELS` 2 (both speakers get the same signal). `SYNTH_GAIN` 0.8.
  The audio thread runs on core 1 at priority 12 with a 12 KB stack in
  PSRAM (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y` in the generated
  sdkconfig).
* **Amplifier**: two NS4150 driven differentially from LOUTP/N and
  ROUTP/N through 0.1 µF + 100 kΩ; both CTRL pins on PA_CTRL (GPIO7),
  pulled down by 10 kΩ. The ES8389 driver switches GPIO7 with the codec,
  so `speaker_enable()` is a no-op.
* **Microphones**: two analog MEMS mics on MIC1P/N and MIC2P/N (schematic,
  not used by firmware). The BSP offers `bsp_audio_codec_microphone_init`.
* **MCLK conflict:** the schematic connects GPIO2 → R95 0 Ω → ES8389 MCLK
  (pin 8). The esp-bsp README says "The MCLK pin on ESP32-S31 is
  unconnected. Recommended sampling rate is 16kHz" and its JSON sets MCLK
  to NC with a 22 050 Hz default. The fork BSP the firmware uses outputs
  MCLK on GPIO2 but tells the codec `use_mclk = false` (clocks derived
  from SCLK), at 48 kHz. korvo-radio confirmed audible playback at 48 kHz
  with SCLK-derived clocks. Piano Coach speaker sound on this board is
  **not yet verified** (only codec initialisation is).
* **I²S data naming:** the BSP's `BSP_AUDIO_I2S_SDOUT` (GPIO5) is named
  from the SoC's side (SoC data out) and lands on the codec's DSDIN; the
  schematic net `I2S_SDOUT` (GPIO6) is the codec's ASDOUT. Same wiring,
  opposite names.

### 4.7 LEDs

* WS2812B (U3) on GPIO37, 5 V supply, BSS138 level shifter (schematic;
  BSP `bsp_led_init/set_rgb/clear` via RMT, GRB order). **Not used by
  firmware.** Note GPIO37 is a strapping pin (JTAG source).
* Red power LED D18 on VCC_5V (not controllable).

### 4.8 Storage (microSD)

* TF slot J7, SDMMC host **slot 0**, 4-bit, 20 MHz (`SDMMC_FREQ_DEFAULT`),
  internal pull-ups enabled, no CD/WP, `unaligned_multi_block_rw_max_chunk_size`
  8. The BSP drives SD_CTRL (GPIO39) low and waits 100 ms for the rail
  before mounting. Mount point `/sdcard`, max 5 open files, 16 KB
  allocation unit, never formats.
* Firmware: `mount_sd()` → `bsp_sdcard_mount(NULL, NULL)`; songs are read
  from the card (`.pianocoach/progress.tsv` stores progress and
  bookmarks). FatFs: long file names in UTF-8 on the heap (255 chars),
  exFAT and GPT enabled by patching the ESP-IDF checkout
  (`tools/patch-idf-exfat.sh`, run by `board.sh` because `PATCH_EXFAT=1`).
* GPIO20–25 are shared with the SPI NAND footprint (U4); "Only one can be
  populated at a time" (BSP comment). ESD: BSDFN2C031U on every line.
* Verified 2026-10-15: card mounted, 2449 songs indexed, Chinese names
  render.

### 4.9 Radio

* On-chip Wi-Fi 6 (2.4 GHz), Bluetooth 5.4 LE + Classic, 802.15.4; PCB
  antenna on the WROOM-3.
* Piano Coach uses only BLE: native controller + NimBLE host, central and
  observer roles only, one connection, preferred MTU 260, bonds persisted
  in NVS, NimBLE memory from external RAM, host task 8 KB stack, NimBLE
  log level WARNING ("NimBLE logs every GATT write at INFO — two console
  lines per MIDI packet"). `bt_controller_start()` is empty —
  `nimble_port_init` brings the controller up itself.
* S31-specific code paths (`build.rs`, `src/ble_midi.rs`): the board is
  **not** `ble_relaxed` (no Wi-Fi, not an original ESP32), so it keeps the
  piano's own 10 ms / 500 ms link and writes with no gap; the cfg
  `nimble_pool_info` is off because the S31's NimBLE port (ESP-IDF master)
  does not export `os_mempool_info_get_next`; notifications are copied
  with `ble_hs_mbuf_to_flat` because `os_mbuf_copydata` is not exported on
  this port.
* Wi-Fi is not used. **BLE has not been tried on this board** (builds only).

### 4.10 Camera and other connectors

* J4 FPC-24P DVP camera (schematic sheet 3): Y2–Y9 = GPIO46–53, PCLK 54,
  XCLK 55, VSYNC 56, HREF 57, SCCB on the shared I²C, IR_CUT and LED+ pins
  routed to test points / 3.3 V. The fork BSP links esp_video and the DVP
  driver (`CONFIG_ESP_VIDEO_ENABLE_SPI_VIDEO_DEVICE=n`), but the firmware
  never opens the camera. korvo-radio: the sensor answers SCCB only once
  the 20 MHz XCLK runs.
* GPIO54–57 are also the JTAG pins (MTDO/MTCK/MTDI/MTMS), so pad JTAG
  conflicts with the camera.

### 4.11 Mechanical

Dimensions drawing (unit mm): overall width 106.50 (bottom edge), overall
height 89.20 (right edge, including the raised top-right section with the
camera, RESET and BOOT), left edge 83.20, top edge of the lower section
75.60, module footprint 22.00 wide. Six M3 mounting holes (schematic
M1–M6). Right edge, top to bottom: USB-C "POWER", USB-C "UART", the "ON"
switch, USB-A "HOST". The LCD header (J14, pin 1 top left) is on the left
edge; speaker connectors Speaker_L and Speaker_R and the four ADC keys on
the bottom edge.

## 5. Memory map and partitions

There is **no `boards/korvo/partitions.csv`**. `tools/board.sh` overrides
`ESP_IDF_GLOB_PARTTABLE_BASE` only when the board has one, so the Korvo
uses the default from `.cargo/config.toml`,
`firmware/piano-coach/partitions/partitions.csv` (its header still says
"piano-tab5"); `sdkconfig.defaults` selects it with
`CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="partitions.csv"`.

| Name | Type | SubType | Offset | Size |
|---|---|---|---|---|
| (bootloader) | — | — | **0x2000** | — |
| (partition table) | — | — | 0x8000 | — |
| nvs | data | nvs | 0x9000 | 24 KB |
| phy_init | data | phy | 0xF000 | 4 KB |
| factory | app | factory | 0x10000 | 12 MB |

Bootloader/table/app offsets are from the build's `flasher_args.json`.
The app image of a 2026-10-16 build
(`target/korvo/riscv32imafc-esp-espidf/release/piano-coach.bin`) is
7 967 216 bytes (≈ 7.6 MiB of the 12 MiB partition). Songs live on microSD, so there is no song-pack
partition. NVS (namespace `piano`, i32 prefs, BLE bonds) is at 0x9000.

Notable `boards/korvo/sdkconfig.defaults` settings (comments quoted):

| Setting | Why |
|---|---|
| `CONFIG_ESPTOOLPY_FLASHMODE_QIO=y`, `FLASHSIZE_16MB=y` | 16 MB QIO flash (the image header says `dio`; the bootloader switches to QIO — normal ESP-IDF behaviour) |
| `CONFIG_FREERTOS_HZ=1000` | "1 kHz tick: MIDI timing needs ms resolution." |
| `CONFIG_ESP_MAIN_TASK_STACK_SIZE=6144`, `CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT=16384`, `CONFIG_ESP_TASK_WDT_TIMEOUT_S=15` | stacks / watchdog (the UI loop feeds the task WDT) |
| `CONFIG_SPIRAM=y`, `SPIRAM_SPEED_250M`, `SPIRAM_USE_MALLOC`, `SPIRAM_XIP_FROM_PSRAM=y`, `FREERTOS_TASK_CREATE_ALLOW_EXT_MEM=y`, `SPIRAM_MEMTEST=n`, `SPIRAM_MALLOC_ALWAYSINTERNAL=0`, `SPIRAM_MALLOC_RESERVE_INTERNAL=65536`, `SPIRAM_ALLOW_BSS_SEG_EXTERNAL_MEMORY`, `SPIRAM_ALLOW_NOINIT_SEG_EXTERNAL_MEMORY` | "PSRAM for framebuffers, the render shadow buffer, songs and — on this board — application thread stacks (internal RAM is kept for the radio, USB and DMA; see src/board/korvo/mod.rs)." |
| `CONFIG_FATFS_LFN_HEAP=y`, `FATFS_MAX_LFN=255`, `FATFS_API_ENCODING_UTF_8=y`, `FATFS_PER_FILE_CACHE=n`, `FATFS_USE_LABEL=y`, `VFS_MAX_COUNT=12` | "microSD: long file names in UTF-8 (the song library is Chinese). exFAT/GPT are enabled by tools/patch-idf-exfat.sh (no Kconfig exists)." |
| `CONFIG_ESP_VIDEO_ENABLE_SPI_VIDEO_DEVICE=n` | "Camera is not used (the BSP still links its driver)." |
| `CONFIG_BT_ENABLED`, `BT_CONTROLLER_ENABLED`, `BT_NIMBLE_ENABLED`, `BT_NIMBLE_MEM_ALLOC_MODE_EXTERNAL`, `BT_NIMBLE_HOST_TASK_STACK_SIZE=8192`, central + observer only, `MAX_CONNECTIONS=1`, `ATT_PREFERRED_MTU=260`, `NVS_PERSIST=y` | "Native BLE controller + NimBLE host, central only." |
| `CONFIG_BT_NIMBLE_LOG_LEVEL_WARNING=y` | "NimBLE logs every GATT write at INFO — two console lines per MIDI packet." |
| `CONFIG_USB_HOST_HUBS_SUPPORTED=y` | "USB-A: native USB 2.0 HS host behind a 500 mA power switch." |

Other values from the generated sdkconfig
(`target/korvo/.../esp-idf-sys/.../out/sdkconfig`): CPU 320 MHz, flash
80 MHz, octal PSRAM 250 MHz, MMU page 64 KB, console UART0 at 115200
(`CONFIG_ESP_CONSOLE_SECONDARY_USB_SERIAL_JTAG=y` is also set, but its
pins are taken by the LCD, see §8), `CONFIG_IDF_INIT_VERSION="6.2.0"`.

**PSRAM-XIP and thread stacks:** `early_init()` sets the pthread default
config to allocate stacks from `MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT`
("Keep internal SRAM for the BLE controller, USB and DMA: application
thread stacks (the UI's is 96 KB) go to PSRAM. With PSRAM-XIP the cache
stays available during flash operations, so that is safe here."). The
audio thread saves and restores this config around its own spawn so later
threads keep the PSRAM policy.

## 6. Build, flash, console

Prerequisites: the repo's nightly Rust with `rust-src`, `ldproxy`,
`espflash` (used as the writer), an ESP-IDF master checkout at
`/home/nws/w/esp32/esp-idf` (or `IDF_PATH`), and the shared ESP-IDF tools
at `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`
(`ESP_IDF_TOOLS_INSTALL_DIR`, set by `board.sh`). The esp-idf-sys/hal/svc
crates are vendored copies with S31 support (`firmware/piano-coach/vendor`,
`[patch.crates-io]` in `Cargo.toml`).

```bash
cd firmware/piano-coach
tools/board.sh korvo build --release                 # target/korvo/riscv32imafc-esp-espidf/release/
tools/board.sh korvo run --release -- --port /dev/ttyUSB0             # flash + monitor (blocks)
tools/board.sh korvo run --release -- --port /dev/ttyUSB0 --no-monitor
# or flash an existing build directly:
IDF_PATH=/home/nws/w/esp32/esp-idf python3 tools/flash-idf.py \
    target/korvo/riscv32imafc-esp-espidf/release/piano-coach --port /dev/ttyUSB0 --no-monitor
```

What `board.sh korvo` sets from `board.env`: `MCU=esp32s31`,
`IDF_PATH` (default above; it must contain `tools/idf.py`),
`RUSTFLAGS=--cfg espidf_time64 --cfg espidf_picolibc` (`PICOLIBC=1`:
"The master toolchain defaults to picolibc; Rust's std must agree"),
runs `tools/patch-idf-exfat.sh $IDF_PATH` (`PATCH_EXFAT=1`, edits
`components/fatfs/src/ffconf.h` in the checkout), `PIANO_FLASHER=idf`,
`ESP_IDF_SDKCONFIG=sdkconfig.korvo`, `CARGO_TARGET_DIR=target/korvo`, and
the feature `board-korvo`. `build.rs` turns `UI=full`, `HAS="usb sd
speaker"` into `cfg(ui_full)`, `cfg(has_usb)`, `cfg(has_sd)`,
`cfg(has_speaker)`. A first build compiles ESP-IDF (10+ minutes).

**Why not espflash:** "espflash cannot package S31 PSRAM-XIP images"
(`board.env`); `flash-idf.py`: "espflash 4.5 cannot locate appdesc in
S31's PSRAM-XIP address range". So `flash.sh` (the cargo runner) hands the
ELF to `tools/flash-idf.py`, which:

1. finds the ESP-IDF build beside the ELF whose bootloader and partition
   table match (refuses to mix builds) and takes ESP-IDF's Python from its
   `CMakeCache.txt`;
2. converts the ELF with `esptool --chip esp32s31 elf2image` using that
   build's flash args, `--elf-sha256-offset 0xb0`, the MMU page size and
   min/max chip revision from the sdkconfig;
3. checks the image fits the app partition;
4. writes bootloader (0x2000), partition table (0x8000) and app (0x10000)
   with one `espflash write-bin --chip esp32s31` each (stub, compression);
   `--writer esptool` uses ESP-IDF's esptool instead, which runs without a
   stub on the S31 (`"stub": false`) and takes about 7 minutes;
5. unless `--no-monitor`, starts `esp_idf_monitor` at the console baud.

`--dry-run` prints the commands. Default flashing baud 921600.

**Console:** UART0 (GPIO58/59) through the CP2102N on the USB-C port
labelled UART, 115200 8N1; it shows the ROM, bootloader and application
logs. Identify the port with
`udevadm info -q property -n /dev/ttyUSBx | grep ID_SERIAL` (AGENTS.md
lists the Korvo as CP2102N, VID 10c4, last seen `/dev/ttyUSB0`; the
Audio Kit's CP2102 can also appear as `ttyUSB0`).

**Auto-program / reset** (schematic sheet 2, U16 BC847BDW1T1G): DTR and
RTS drive EN and GPIO61 (BOOT) with the usual table

| DTR | RTS | EN | GPIO61 |
|---|---|---|---|
| 1 | 1 | 1 | 1 |
| 0 | 0 | 1 | 1 |
| 1 | 0 | 0 | 1 |
| 0 | 1 | 1 | 0 |

so esptool/espflash enter download mode without buttons. When reading
logs with pyserial, open the port with DTR and RTS false (otherwise the
board resets); reset by pulsing RTS (korvo-radio: DTR low = run, pulse RTS
high→low). Do not use `espflash monitor --before no-reset`: it leaves the
chip in the ROM bootloader.

**Manual download mode:** hold BOOT (GPIO61 low), press and release RESET,
release BOOT. Boot-mode strapping (S31 datasheet table 3-3): GPIO61 = 1 →
SPI boot; GPIO61 = 0 and GPIO60 = 1 → joint download boot (UART,
USB-Serial-JTAG, USB-OTG, GPSPI); GPIO61 = 0 and GPIO60 = 0 is invalid.
Both have internal weak pull-ups (external 10 kΩ pull-ups R101/R102 are
not fitted).

**Setting a pref from the PC:** build an NVS image with ESP-IDF's
`nvs_partition_gen.py generate prefs.csv nvs.bin 0x6000` (namespace
`piano`, i32 values) and write it at **0x9000** (the `nvs` offset above).
On this board `espflash write-bin` works for that (raw binary); it
replaces all prefs and BLE bonds. Prefs of interest: `transport` (1 =
BLE, else USB), `spkvol`.

**PC preview:** `tests/ui_preview` renders the full UI at the Korvo's
geometry (`("korvo", 800, 480, 0.625)`), see `korvo-*.png`.

## 7. Piano Coach support

* **UI class:** full (`ui.rs`, `ui/*.slint`): library, play page with
  falling notes / sheet music, settings, search, song globe; drawn at
  0.625 scale. `CAPS`: name "ESP32-S31-Korvo-1", backlight false,
  power_latch false, usb_midi true, ble_midi true, buttons true, touch
  true, wake_hint "RESET".
* **Transports:** USB-A host (default) or BLE, one per boot: `main.rs`
  picks USB unless pref `transport` = 1; switching in Settings restarts
  the board (`reboot()` = `esp_restart`).
* **Library:** microSD at `/sdcard` (bookmarks and scores in
  `.pianocoach/progress.tsv` on the card). Bookmarks (C8) are
  supported on this board.
* **Speaker:** Settings → Notes sound from Piano / Speaker / Both, volume
  slider and VOL± keys.

Verified on hardware (2026-10-15, commit 8242e3f, ESP-IDF master v6.1-dev
4d59230ddff):

| Check | Result |
|---|---|
| Boot, display at 0.625 scale, dark UI | ✔ ("looks right") |
| microSD mount, library indexed (2449 songs) | ✔ |
| USB-A (HS host): piano enumerates (`CHMidi-2.3`), "Light all keys", Learn LEDs | ✔ |
| Touch | ✔ except the dead strip at ≈ 50–62 % of the width |
| ES8389 initialised (48 kHz stereo); four ADC keys created | ✔ — sound and key presses **not checked** |
| BLE, speaker playback, PLAY / SET / VOL± presses, song globe, shut-down / RESET wake | **not verified** |

Since then (2026-10-16) the Korvo has only been **built** after each
shared change (sheet music, piano-key remote and boot song, shuffle-play,
category stepping, stale-USB-queue drop, NimBLE cfg gate). AGENTS.md:
"The Korvo has the latest firmware but had no microSD card in at the last
boot." Not tried on the Korvo: the piano-key remote, Follow mode,
boot-into-Follow with a 儿时回忆 song, shuffle-play, F1/E1 category
stepping on its card.

Known limits: the touch dead strip (use SET); no brightness control; no
true power-off; the camera, microphones, RGB LED, Wi-Fi and the BOOT key
are unused.

## 8. Quirks and lessons learned

* **ESP-IDF master only.** No ESP-IDF release supports the S31;
  `board.sh` refuses an `IDF_PATH` without `tools/idf.py`. The checkout is
  patched in place for exFAT/GPT (idempotent). The toolchain defaults to
  picolibc, so Rust needs `--cfg espidf_picolibc`.
* **Flashing needs `tools/flash-idf.py`** (espflash cannot package
  PSRAM-XIP S31 images). The bootloader goes to **0x2000**, not 0x0.
  espflash's own `flash` command must not be used; `espflash write-bin`
  per region is fine.
* **Panel timing**: use 18 MHz / 40-40-48 / 23-32-13 with falling-edge
  latch; the upstream esp-dev-kits 26 MHz timings roll.
* **Exactly two framebuffers**, and `present` must wait for the GDMA
  frame-switch callback, not VSYNC.
* **Rotation**: the BSP refuses anything but 0; the panel is used
  landscape as mounted.
* **Shared I²C bus**: touch, codec and camera SCCB share GPIO0/1 (I2C_NUM_0,
  created by the display/touch bring-up). The speaker codec waits for
  `DISPLAY_UP` before touching it.
* **ES8389 address**: 0x20 in the driver/schematic is the 8-bit form; a
  bus scan sees 0x10.
* **Button ladder on a negative ADC input**: idle reads raw 0; use the
  BSP's S31 calibration (regi2c writes) or korvo-radio's
  `mv = 2000 − 4000·code/4393`, not the generic `adc_cali` path.
  Espressif's esp-bsp has VOL+/VOL− swapped relative to the schematic.
* **USB host on one core**: keep install, event loop and uninstall on the
  same core (BSP task pinned to core 1).
* **Thread stacks in PSRAM** (`early_init`), safe only because of
  PSRAM-XIP; code that spawns threads with its own pthread config must
  restore the board's policy afterwards (see `audio.rs`).
* **Strapping and reserved pins in use** (S31 datasheet §2.3.4, table
  3-1): GPIO36 (LCD R7, VDD_SPI strap; pulled up inside the module),
  GPIO37 (WS2812, JTAG-source strap), GPIO60 (LCD MOSI, boot mode / ROM
  print strap), GPIO61 (LCD SCK and BOOT key, boot mode strap). GPIO33/34
  (USB Serial/JTAG) carry LCD R4/R5, so there is **no USB Serial/JTAG
  console** on this board; GPIO54–57 (pad JTAG) are the camera. Don't
  drive GPIO60/61 low from a sub-board during reset.
* **korvo-radio's pin notes** call GPIO38 and GPIO40 "boot-mode strapping
  pins"; the S31 datasheet lists only GPIO36, 37, 60 and 61. Follow the
  datasheet.
* **SD power is active low** (GPIO39 low = powered); the card has no
  card-detect line.
* **NimBLE on the S31 port** lacks `os_mbuf_copydata` and
  `os_mempool_info_get_next`; use `ble_hs_mbuf_to_flat` and keep
  `nimble_pool_info` off.
* **Only one BLE central at a time** on the piano — power off the Tab5 or
  other boards when testing BLE here.
* **The power switch** (J3, "ON") cuts VCC_5V; a board that "vanished"
  may just be switched off (the CP2102N stays powered from USB).

## 9. Discrepancies and open questions

| Topic | Sources | Firmware |
|---|---|---|
| Touch controller name | BSPs, commit: GT1151; `doc/piano_coach_firmware.md`: GT1158; korvo-radio: product ID "1158" | GT1151 driver at 0x14, works |
| ES8389 address | schematic/driver 0x20 vs bus 0x10 | 0x20 to esp_codec_dev (= 0x10 on the wire) |
| MCLK | schematic: GPIO2 wired; esp-bsp: "unconnected", 16 kHz recommended | GPIO2 driven, codec `use_mclk = false`, 48 kHz; sound unverified |
| VOL+ / VOL− levels | schematic + fork: VOL+ 0.38 V, VOL− 0.82 V; esp-bsp: swapped | fork's assignment; presses unverified |
| Panel timing | esp-dev-kits upstream 26 MHz; fork/esp-bsp/korvo-radio 18 MHz | 18 MHz |
| LCD DISP pin | esp-bsp: `BSP_LCD_RGB_DISP` GPIO38; fork + schematic: GPIO38 is LCD_CS, DISP_EN NC | not driven |
| USB pins | esp-bsp JSON: `BSP_USB_POS/NEG` GPIO20/19 (actually SD_D0 and LCD R3) | dedicated USB_DP/DM pads; the JSON entry is wrong |
| Console bridge | korvo-radio README: FT232R; korvo-radio `01-board.md`, schematic, AGENTS.md: CP2102N | CP2102N |
| Strapping pins | korvo-radio: GPIO38/40/60/61; S31 datasheet: GPIO36/37/60/61 | — (datasheet) |
| Flash time | `flash-idf.py` docstring: ~16 s for 7.5 MB with espflash; firmware doc: ~1.5 min for the app | unmeasured here |
| ESP-IDF version | verified table: master v6.1-dev 4d59230ddff; current build: `IDF_INIT_VERSION` 6.2.0 (the version the sdkconfig was created with) | whatever the checkout is |
| Partition table | `boards/README.md` lists a per-board `partitions.csv` as optional; the Korvo has none | `partitions/partitions.csv` (header says "piano-tab5") |
| Unknown | bench-unit MAC, chip revision, CP2102N PID; which camera sensor is on the bench unit; speaker loudness at `SYNTH_GAIN` 0.8 | — |

## 10. Sources

In this repo:

* `AGENTS.md`; `README.md`; `doc/piano_coach_firmware.md` (board table,
  Korvo buttons, shut down, build/flash, "Verified on hardware",
  "Known limits")
* `firmware/piano-coach/boards/korvo/board.env`,
  `boards/korvo/sdkconfig.defaults`, `boards/korvo/esp-idf-components.toml`,
  `boards/README.md`
* `firmware/piano-coach/partitions/partitions.csv`, `.cargo/config.toml`,
  `Cargo.toml`, `build.rs`, `flash.sh`, `tools/board.sh`,
  `tools/flash-idf.py`, `tools/patch-idf-exfat.sh`
* `firmware/piano-coach/src/board/korvo/mod.rs`, `src/board/mod.rs`,
  `src/board/contract.rs`, `src/bindings.h`, `src/main.rs`, `src/ui.rs`,
  `src/audio.rs`, `src/usb_midi.rs`, `src/ble_midi.rs`
* `firmware/piano-coach/components/esp32_s31_korvo/` (`esp32_s31_korvo.c`,
  `esp32_s31_usb.c`, `esp32_s31_adc_calibration.c`, `include/bsp/*.h`,
  `idf_component.yml`, `CMakeLists.txt`)
* Build outputs of 2026-10-16 under `firmware/piano-coach/target/korvo/`
  (generated `sdkconfig`, `flasher_args.json`, `partitions.csv`,
  managed-component headers `es8389_codec.h`, `esp_lcd_touch_gt1151.h`)
* `doc/boards/vendor/korvo/esp32-s31-korvo-1-schematics.pdf`,
  `doc/boards/vendor/korvo/esp32-s31-korvo-1-dimensions.pdf`
* git history: commit 8242e3f (Korvo port) and later commit messages

Outside the repo (not copied):

* `/home/nws/w/esp32/doc/ESP32-S31/esp32-s31_datasheet_en.pdf` (v0.5,
  preliminary: features, strapping pins, GPIO restrictions)
* `/home/nws/w/esp32/doc/ESP32-S31/esp32-s31-wroom-3_wroom-3u_datasheet_en.pdf`
  (v0.7: pin table)
* `/home/nws/w/esp32/doc/ESP32-S31/esp32-s31-korvo-1-pcb-layout.pdf` (not
  needed for this document)
* `/home/nws/w/esp32/esp-bsp/bsp/esp32_s31_korvo_1/` (README, JSON,
  `include/bsp/esp32_s31_korvo_1.h`, `src/bsp_button.c`, `src/bsp_audio.c`,
  `src/bsp_display.c`, `Kconfig`)
* `/home/nws/w/esp32/esp-dev-kits/examples/esp32-s31-korvo/` (README;
  upstream `common_components/esp32_s31_korvo` timings)
* `/home/nws/w/esp32/esp32-s31/korvo-radio/` (README, `doc/01-board.md`,
  `03-audio.md`, `05-touch.md`, `06-usb.md`, `10-lcd-known-issues.md`) —
  independent bare-metal Rust demo, verified I²C addresses, touch
  protocol, ADC ladder interpretation
* `/home/nws/w/esp32/esp_codec_dev/platform/audio_codec_ctrl_i2c.c`
  (8-bit → 7-bit address shift)
