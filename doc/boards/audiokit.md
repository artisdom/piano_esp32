# Ai-Thinker ESP32-Audio-Kit (ESP32-A1S, ES8388) — hardware reference

## 1. Summary

The ESP32-Audio-Kit is Ai-Thinker's audio development board for the
ESP32-A1S module: an original ESP32 (rev 3) with 4 MB flash, 8 MB PSRAM
and an ES8388 audio codec inside the module, plus on the board two
speaker amplifiers, a headphone jack, a line-in jack, two microphones, a
microSD slot, six keys, LEDs, a Li-ion charger and a CP2102 USB-UART
bridge. It has no display. Piano Coach runs **headless** on it: the six
keys, short tones on the speaker, the green LED and a phone web page
(Wi-Fi access point with captive portal) drive the app; songs come from the
microSD card; the piano is reached over BLE only (the board has no USB
host). The speaker can also play the songs through the built-in additive
piano synth. Ported and verified on 2026-10-16.

| Item | Value |
|---|---|
| Vendor / product | Ai-Thinker (安信可) ESP32-Audio-Kit, ES8388 version |
| Board revision | Board spec V2.2 (spec doc V1.1, 2021-06); `board.env` says "V2.2/V2.3"; the bench unit's PCB revision was not recorded |
| Module | ESP32-A1S, ES8388 version, module V2.3 (SMD-38, 31.5 × 19.0 × 3.1 mm) |
| Board dir | `firmware/piano-coach/boards/audiokit` |
| Cargo feature | `board-audiokit` |
| MCU | ESP32 (Xtensa LX6 dual core, 240 MHz), chip rev 3 (firmware doc: "rev 3.1"; `CONFIG_ESP32_REV_MIN_3=y`) |
| Rust target / toolchain | `xtensa-esp32-espidf`, esp-rs toolchain `esp198` (`espup install --name esp198 --toolchain-version 1.98.1.0`) |
| ESP-IDF | v5.5.3 (`ESP_IDF_VERSION` in `board.env`) |
| Flash | 4 MB (module spec; `CONFIG_ESPTOOLPY_FLASHSIZE_4MB`), DIO, 40 MHz |
| PSRAM | 8 MB (64 Mbit, module spec), quad, 40 MHz; the original ESP32 maps 4 MB, the rest is only reachable through bank switching (himem), which the firmware does not use |
| Codec | ES8388 (inside the module), I²C 7-bit address 0x10 |
| USB-UART | CP2102 on the "UART" micro-USB port, USB ID 10c4:ea60 (per `doc/boards/README.md`; not re-read in this session), seen as `/dev/ttyUSB*` |
| Console | UART0, 115200 baud |
| Bench unit identifiers | MAC and CP2102 serial string not recorded |

## 2. Feature overview

| Component | Part | Function | Bus / signals | Address / pins | Used by Piano Coach |
|---|---|---|---|---|---|
| SoC module | ESP32-A1S (ES8388 ver., V2.3) | ESP32 + 4 MB flash + 8 MB PSRAM + ES8388 | — | — | yes |
| Audio codec | ES8388 (in module) | 2 ADC + 2 DAC, mic and headphone amps | I²C (control) + I²S0 (audio) | I²C 0x10 on SDA 33 / SCL 32; MCLK 0, BCLK 27, LRCK 25, DOUT 26; DIN 35 (not used) | yes (DAC only) |
| Speaker amplifiers | U4 / U5; part "NS4150B" per `doc/piano_coach_firmware.md`, not legible in the vendor schematic image; board spec: "LOUT/ROUT 3 W" | two mono class-D amps, one per channel, to speaker connectors J3 (left) and J4 (right) | analog from codec; shared enable `CTRL` | enable GPIO21 (high = on; pulled down by R51) | yes |
| Headphone jack | J2, 3.5 mm | stereo out from HPOUTL/HPOUTR | analog | detect GPIO39 (schematic, R36 pull-up) | no (output not tested, detect not read) |
| Line-in jack | J1, 3.5 mm | stereo in to LINEINL/LINEINR = ES8388 LIN2/RIN2 (shared with MIC2) | analog | ADF: AUXIN detect GPIO12 | no |
| Microphones | MIC1, MIC2 (electret, MBIAS/HBIAS bias); MIC3/MIC4 footprints (VDD/OUT/GND) | analog mics to ES8388 LIN1/RIN1 and LIN2/RIN2 | analog | — | no |
| Keys | K3…K8 = KEY1…KEY6 | user keys, active low | GPIO | 36, 13, 19, 23, 18, 5 | yes |
| BOOT key | K1 | pulls IO0 and IO2 low through Q5/Q6 | — | GPIO0, GPIO2 | no (download mode only) |
| RST key | K2 | pulls EN (RST) low | — | EN | reset only |
| Green LED | LED4 | status | GPIO | GPIO22 (schematic: active low — see §4.5) | yes |
| Second LED | LED5 ("BLUE_LED" in the ADF package) | — | GPIO | GPIO19, shared with KEY3 | no |
| Power LED | LED3 | on while VCC3V3 is present | — | — | no |
| Charge LEDs | LED1, LED2 | charger CHRG / STDBY | — | — | no |
| microSD | SD1 (micro SD/TF), card detect pin | library storage | SDMMC slot 1, 1-bit | CLK 14, CMD 15, D0 2 (D1 4, D2 12, D3 13, detect 34 not used) | yes |
| DIP switch | S1, 5 positions | routes IO13 / IO15 to KEY2, SD or JTAG | — | see §3.2 | must be set right |
| JTAG header | P1 | MTDO, MTCK, MTDI (IO12), MTMS (IO14) | JTAG | via DIP 4/5 | no |
| Pin headers | P2, P3, P4 | IO21, IO22, IO19, IO23, IO18, IO5; 3V3, RXD0, TXD0, RST, IO0, GND | — | — | no |
| USB-UART | U3, CP2102 | console, flashing, auto-reset (DTR/RTS → Q2/Q3/Q4) | USB2 micro-USB | UART0 GPIO1/GPIO3 | yes |
| Power input | USB1 micro-USB "POWER" (VBUS only) | 5 V in | — | — | yes (spec: POWER port, or both ports; which port powered the bench unit was not recorded) |
| Buck regulator | U2 (part unknown) | 5 V → 3.3 V (schematic: 3.38 V from 510k/110k divider) | — | — | — |
| Li-ion charger | U1 (part unknown; VCC/CE/BAT/TEMP/PROG/CHRG/STDBY pins) | charges BT1 from VBUS | — | — | no |
| Battery | BT1 connector, 3.7 V Li-ion | Q1 (P-MOSFET) switches VBAT onto VCC5V when VBUS is absent | — | — | no (not tested) |
| Antenna | PCB antenna or IPEX (two board variants per spec) | Wi-Fi / BT | — | — | yes; bench variant not recorded |

## 3. Pin map

### 3.1 GPIOs

"Firmware" = driven or read by Piano Coach. Sources: **code** =
`src/board/audiokit/mod.rs` / `components/audiokit_bsp/audiokit_bsp.c`;
**ADF** = vendor ESP-ADF board package `esp32_audio_kit_v2_3`
(`board_def.h`, `board_pins_config.c`); **mod-sch** = ESP32-A1S V2.3 module
schematic; **mod-spec** = module spec V2.3 (EN); **bd-spec** / **bd-sch** =
board spec V2.2 and the schematic image on its page 15.

| GPIO | Function on this board | Dir / level | Peripheral | Firmware | Source |
|---|---|---|---|---|---|
| 0 | ES8388 MCLK (`ES_I2S_MCLK`, 10 kΩ pull-up R42 in module); boot strap; BOOT key K1 and auto-reset Q3 pull it low; header P3 | out (CLK_OUT1) | I²S0 MCLK | yes | code, mod-sch, ADF, bd-sch |
| 1 | U0TXD → CP2102 | out | UART0 | console | mod-spec, bd-sch |
| 2 | SD DAT0 (pull-up on card side); boot strap (must be low for download); K1 pulls it low through Q6, auto-reset Q4 too | bidir | SDMMC slot 1 | yes | code, bd-sch, mod-spec |
| 3 | U0RXD ← CP2102 | in | UART0 | console | mod-spec, bd-sch |
| 4 | SD DAT1 | — | SDMMC | no (1-bit mode) | bd-sch |
| 5 | KEY6 (VOL−) | in, active low | GPIO, internal pull-up | yes | code, bd-spec, ADF |
| 6–11 | module flash | — | SPI flash | (system) | mod-sch |
| 12 | SD DAT2 (pull-up on card side); JTAG MTDI on P1; ADF: AUXIN (line-in) detect. Also the ESP32's MTDI flash-voltage strap | — | — | no | bd-sch, ADF |
| 13 | KEY2 (MODE) via DIP 1; SD DAT3 via DIP 2; JTAG MTCK via DIP 4. RTC GPIO — deep-sleep wake pin in `power_off()` | in, active low | GPIO, internal pull-up | yes | code, bd-spec, ADF |
| 14 | SD CLK; JTAG MTMS on P1 | out | SDMMC slot 1 | yes | code, bd-sch |
| 15 | SD CMD via DIP 3; JTAG MTDO via DIP 5 | bidir | SDMMC slot 1 | yes | code, bd-spec |
| 16, 17 | module PSRAM (`PSRAM_CS`, `PSRAM_CLK` nets) | — | PSRAM | (system) | mod-sch |
| 18 | KEY5 (VOL+); header P2 | in, active low | GPIO, internal pull-up | yes | code, bd-spec, ADF |
| 19 | KEY3 (SET); also cathode of LED5 (ADF "BLUE_LED"); header P2 | in, active low | GPIO, internal pull-up | yes (as key) | code, bd-spec, bd-sch, ADF |
| 21 | amplifier enable `ShutDown`/`CTRL` for U4 and U5 (R46 series, R51 pull-down); header P2 | out, high = amps on | GPIO (driven by esp_codec_dev as `pa_pin`) | yes | code, bd-sch, ADF |
| 22 | green LED4 cathode (VDD3V3 → R14 → LED → IO22); header P2 | out; schematic: low = lit | GPIO | yes (high = "on" in code) | code, bd-sch, ADF |
| 23 | KEY4 (PLAY); header P2 | in, active low | GPIO, internal pull-up | yes | code, bd-spec, ADF |
| 25 | ES8388 LRCK (WS) | out | I²S0 | yes | code, mod-sch, ADF |
| 26 | ES8388 DSDIN (ESP32 → codec data) | out | I²S0 DOUT | yes | code, mod-sch, ADF |
| 27 | ES8388 SCLK (BCLK) | out | I²S0 | yes | code, mod-sch, ADF |
| 32 | ES8388 I²C SCL (10 kΩ pull-up R35 to AVCC in module, 100 Ω series) | out | I²C0 | yes | code, mod-sch, ADF |
| 33 | ES8388 I²C SDA (10 kΩ pull-up R34 to AVCC in module, 100 Ω series) | bidir | I²C0 | yes | code, mod-sch, ADF |
| 34 | microSD card detect (`SD_Detect`, R18 pull-up), input only | in | — | no | bd-sch, ADF |
| 35 | ES8388 ADC data out (`ES_I2S_ADSDOUT`), input only | in | I²S DIN | no (`din = I2S_GPIO_UNUSED`) | mod-sch, ADF |
| 36 | KEY1 (REC) — the schematic's `KEY_AD` resistor-ladder node (see §4.4), input only, no internal pull-up | in, active low | GPIO | yes | code, bd-spec, bd-sch, ADF |
| 39 | headphone detect `HP_Detect` (R36 pull-up to VDD3V3), input only | in | — | no | bd-sch, ADF |
| EN | RST key K2, auto-reset Q2 | in, low = reset | — | — | bd-sch, bd-spec |

Strapping notes (ESP32 chip behaviour): GPIO0 and GPIO2 low at reset =
download mode (module spec table 9). GPIO12 (MTDI) selects the flash
voltage at reset and is wired to SD DAT2, which has a pull-up on the card
side in the board schematic; how the module avoids the 1.8 V strap (eFuse
or unpopulated resistor) is unknown — the bench unit boots normally with a
card inserted.

### 3.2 DIP switch S1 (IO13 / IO15 routing)

From board spec table 9 (translated) and the schematic (S1 pins 1–5 /
10–6):

| Switch | Name | When ON |
|---|---|---|
| 1 | IO13-KEY2 | IO13 connects to KEY2 |
| 2 | IO13-DATA3 | IO13 connects to the SD socket's DATA3 |
| 3 | IO15-CMD | IO15 connects to the SD socket's CMD |
| 4 | IO13-MTCK | IO13 connects to JTAG MTCK |
| 5 | IO15-MTDO | IO15 connects to JTAG MTDO |

Positions Piano Coach needs (inferred from the pins the firmware uses;
the bench unit's actual positions were not recorded): **1 ON** (KEY2),
**3 ON** (SD CMD), **2 OFF** (1-bit SD does not use DAT3; inference: with 2 and
1 both on, pressing KEY2 would pull the card's DAT3 low), **4 and 5 OFF** (no
JTAG). JTAG debugging would cost KEY2 and the SD card.

### 3.3 Module pins (ESP32-A1S V2.3, 38 pins)

Module spec table 8, with the ES8388 signal each audio pin carries:

| Pin | Name | Signal | Pin | Name | Signal |
|---|---|---|---|---|---|
| 1 | GND | | 20 | GND | |
| 2 | 3V3 | 3.0–3.6 V, > 500 mA | 21 | LINEINR | ES8388 RIN2 |
| 3 | SENSOR_VN | GPI39 | 22 | LINEINL | ES8388 LIN2 |
| 4 | SENSOR_VP | GPI36 | 23 | NC | (was SPORP on the AC101 module) |
| 5 | IO34 | GPI34 | 24 | SPORN | ES8388 ROUT1 |
| 6 | IO0 | GPIO0 = ES8388 MCLK | 25 | NC | (was SPOLP on the AC101 module) |
| 7 | IO14 | | 26 | SPOLN | ES8388 LOUT1 |
| 8 | IO12 | | 27 | HPOUTL | ES8388 LOUT2 |
| 9 | IO13 | | 28 | HPOUTR | ES8388 ROUT2 |
| 10 | IO15 | | 29 | IO5 | |
| 11 | IO2 | | 30 | IO18 | |
| 12 | IO4 | | 31 | IO23 | |
| 13 | HBIAS | 1 kΩ to AVCC | 32 | IO19 | |
| 14 | MIC2N | ES8388 RIN2 | 33 | IO22 | |
| 15 | MIC1N | ES8388 RIN1 | 34 | IO21 | |
| 16 | MBIAS | 1 kΩ to AVCC | 35 | EN | high = run |
| 17 | MIC1P | ES8388 LIN1 | 36 | TXD0 | GPIO1 |
| 18 | MIC2P | ES8388 LIN2 | 37 | RXD0 | GPIO3 |
| 19 | GND | | 38 | GND | |

The module schematic's own labels differ in places (pins 27/28 are
labelled LINEINL/LINEINR next to LOUT2/ROUT2; MIC1P/MIC1N on pins 15/17
are swapped relative to the spec); the spec table above is the clearer
source. The board schematic (dated 2019-04-09, drawn for the older AC101
module) still names pins 23/25 SPORP/SPOLP and feeds the amplifiers' +
inputs from them — on the V2.3 module those pins are NC (see §4.3).

## 4. Subsystems

### 4.1 Power and battery

* Two micro-USB ports: **POWER** (USB1, VBUS and GND only) and **UART**
  (USB2, CP2102). Either powers the board. Board spec: 4.7–5.3 V, > 1 A;
  it recommends 5 V with ≥ 1 A peak, or both ports connected.
* 5 V (VCC5V) → buck U2 → VCC3V3 (≈ 3.38 V). The amplifiers run from
  VDD5V (VCC5V through L3), the module from VDD3V3 (VCC3V3 through L2).
* Li-ion: charger U1 on VBUS charges BT1 (3.7 V cell, spec); LED1/LED2
  show CHRG/STDBY. Q1 connects VBAT to VCC5V when VBUS is absent. Battery
  operation is not tested with Piano Coach.
* No power switch or power latch (`CAPS.power_latch = false`).
  `power_off()` in `mod.rs` turns the LED off and enters deep sleep with
  ext0 wake on KEY2 (GPIO13, RTC pull-up), but it is only called from the
  full and mini UIs' Sleep items — the headless UI never calls it, so on
  this board it exists only for the board contract and is unverified.

### 4.2 USB-UART, console, reset

* CP2102 (U3) on UART0 (GPIO1 TX, GPIO3 RX), 115200 baud console
  (`CONFIG_ESP_CONSOLE_UART_DEFAULT`).
* Auto-reset/auto-download: DTR/RTS drive cross-coupled transistors Q2/Q3
  on EN and IO0 (the usual ESP32 circuit), and Q4 also pulls IO2 low
  (board schematic; exact wiring of Q4 not legible). espflash enters
  download mode by itself.
* Manual download mode: hold K1 (BOOT: pulls IO0 and IO2 low) and press
  K2 (RST). Silkscreen names of K1/K2 on the board are not recorded.
* Opening the port with DTR/RTS asserted resets the board; open it with
  both false to watch logs without resetting (AGENTS.md). Holding RTS
  keeps it in reset.

### 4.3 Audio (ES8388 + amplifiers)

What the firmware does (`components/audiokit_bsp/audiokit_bsp.c`,
`src/audio.rs`, `src/board/audiokit/mod.rs`):

* **I²C**: new-style `i2c_master` bus on port 0, SDA GPIO33, SCL GPIO32,
  internal pull-ups on (the module also has 10 kΩ pull-ups).
  esp_codec_dev's I²C clock default.
* **ES8388 address**: the BSP passes `ES8388_CODEC_DEFAULT_ADDR` = **0x20**,
  which is the 8-bit write form; esp_codec_dev's I²C control layer shifts
  it (`addr >> 1`) to the 7-bit address **0x10**. Both numbers are right —
  do not "fix" one to the other.
* **I²S0**, ESP32 master, ES8388 slave (`master_mode = false`): MCLK GPIO0,
  BCLK GPIO27, WS GPIO25, DOUT GPIO26, no DIN. GPIO0 is fixed by the
  module (its `ES_I2S_MCLK` net is wired to GPIO0 inside the module). The
  BSP comment says the ESP32 can put MCLK only on GPIO0; the ADF package's
  `i2s_mclk_gpio_select` shows GPIO0/1/3 are possible on the chip — the
  real constraint is the module wiring.
* The BSP initialises I²S at 48 kHz mono 16-bit; that is a placeholder:
  `esp_codec_dev_open()` (in `audio.rs`) reconfigures the channel
  (`i2s_channel_reconfig_std_slot/clock` in esp_codec_dev's
  `platform/audio_codec_data_i2s.c`) to
  **24 kHz, 16-bit, 2 channels** (`AUDIO_OUT_RATE = 24_000`,
  `AUDIO_CHANNELS = 2`). The synth renders mono at 24 kHz; `audio.rs`
  duplicates each sample into both channels (upsample factor 1).
* **Stereo is required**: the two speaker amplifiers take the left and
  right channel respectively; a mono stream would reach only one.
* **Codec driver**: esp_codec_dev ~1.5 (1.5.11 in the build),
  `es8388_codec_new` with `codec_mode = DAC`, `pa_pin = 21`,
  `pa_reverted = false`, `hw_gain = {pa_voltage 5.0, codec_dac_voltage
  3.3}`. The driver switches GPIO21 (amplifier enable) itself on
  enable/disable/close, so `speaker_enable()` is a no-op.
* **Register fix**: at creation the driver writes DACCONTROL24/25 (0x2E/
  0x2F, LOUT1/ROUT1 volume) = 0x1E (0 dB) and DACCONTROL26/27 (0x30/0x31,
  LOUT2/ROUT2) = 0x00 (−30 dB per the driver's comment). On this board
  that left the speaker quiet (−30 dB per the driver comment). The BSP then writes **0x30 = 0x31 =
  0x21** (+3 dB per the driver's comment, the maximum) and logs the
  read-back. It must go through the control interface
  (`ctrl_if->write_reg`): the ES8388 driver has no `set_reg`, so
  `esp_codec_dev_write_reg()` silently does nothing for this codec. The
  driver writes those registers only at creation, so once is enough.
  Result on the bench: "loud and clean" (`doc/piano_coach_firmware.md`).
* **Volume**: `esp_codec_dev_set_out_vol(0…100)` → the driver's DAC
  digital volume (DACCONTROL4/5). Default 70, NVS key `spkvol`, ±10 per
  KEY5/KEY6 press or from the web page.
* **Gain**: `SYNTH_GAIN = 1.6` (twice the synth default): songs peak around
  −6 dBFS on the small speakers; loud chords saturate softly instead of
  clipping.
* **Power management**: the audio thread (core 1, priority 12, 12 KB stack
  in PSRAM) opens the codec only while synth voices sound and closes it
  (amplifiers off) after 2 s of silence — an open, idle amplifier hisses.
  It renders 256-sample blocks; the I²S DMA paces the loop and the thread
  logs its CPU share every 1000 blocks.
* **Where notes sound**: NVS `sound` 0 = piano, 1 = speaker, 2 = both
  (KEY1 hold or the web page). Feedback tones always use the speaker.

Hardware the firmware does not use (schematics):

* Amplifiers U4 (left, to J3 LOP/LON) and U5 (right, to J4 ROP/RON), shared
  `CTRL` enable from GPIO21 through R46 with pull-down R51 (amps off at
  reset), supply VDD5V.
* **Amplifier inputs — unresolved.** The board schematic (2019, AC101
  era) feeds each amp's InP from SPOLP/SPORP (module pins 25/23) and InN
  from SPOLN/SPORN (pins 26/24). On the V2.3 ES8388 module pins 23/25 are
  NC and pins 24/26 carry ROUT1/LOUT1. Ai-Thinker's V2.3 replacement note
  says the outputs are now "right: HPOUTR (+) and SPORN (−); left: HPOUTL
  (+) and SPOLN (−)", i.e. ROUT2/LOUT2 as + and ROUT1/LOUT1 as −. The
  firmware's statement that the speaker amps "hang on LOUT2/ROUT2" is an
  inference from the register fix working; the actual amplifier input
  wiring of ES8388-version boards is not shown in the available schematic
  image.
* Headphone jack J2 is on HPOUTL/HPOUTR = LOUT2/ROUT2 (module spec, board
  schematic) — the same outputs the fix raised to +3 dB. Headphone output
  and level are untested. Headphone detect on GPIO39 (pulled up by R36)
  is not read, so the amplifiers stay on with headphones plugged in.
* Line-in J1 → module LINEINL/LINEINR = ES8388 LIN2/RIN2. The replacement
  note: line-in and MIC2 share these inputs and cannot be used at the same
  time; MIC1 or MIC2 can be configured as line-in in software.
* Microphones MIC1 (LIN1/RIN1, bias MBIAS) and MIC2 (LIN2/RIN2, bias
  HBIAS); footprints MIC3/MIC4 (population unknown). ADC data reaches the
  ESP32 on GPIO35. ADF default ADC input: `AUDIO_HAL_ADC_INPUT_LINE2`.

### 4.4 Keys

| Key | GPIO | ADF name | Press | Hold (≥ 600 ms) |
|---|---|---|---|---|
| KEY1 | 36 | REC | restart the song | notes from piano → speaker → both (1–3 beeps) |
| KEY2 | 13 | MODE | Learn → Follow → Listen (1, 2, 3 rising beeps) | hands Both → Right → Left (octave / high / low beep) |
| KEY3 | 19 | SET | next song in the folder (starts at once) | previous song |
| KEY4 | 23 | PLAY | play / pause | stop |
| KEY5 | 18 | VOL+ | speaker volume +10 | tempo +10 % |
| KEY6 | 5 | VOL− | speaker volume −10 | tempo −10 % |

* All active low (pressing connects the key to GND). Driver:
  `src/board/key.rs`, polled every ~15 ms from the headless loop; 25 ms
  debounce, 600 ms hold, 280 ms double-tap window. A tap is reported only
  after the double-tap window, and `poll_buttons()` maps both tap and
  double tap to one `Keypad(n)` — two presses within 280 ms count as one.
* Pull-ups: `key.rs` enables the internal pull-up on GPIO13/19/23/18/5 and
  skips GPIO36 (input-only, no internal pull-up on the ESP32). The board
  spec's key table says "no external pull-up resistor" for every key, but
  the board schematic shows an external network: VDD3V3 → R52 → R54 → a
  ladder R55…R59, with each key node joined to the ladder through
  R60…R64 and to its GPIO through R66…R70. KEY1 relies entirely on that
  chain.
* **GPIO36 is a resistor-ladder node.** In the schematic, IO36 sits on
  `KEY_AD` (between R52 and R54, via R53, with C41), so it is an analog
  ladder input that sees every key: KEY1 pulls it lowest, KEY2…KEY6 pull it
  down through more of the ladder. The firmware reads it digitally as
  KEY1. The six keys worked on the bench, but whether KEY2…KEY6 can also
  register as KEY1 (cross-talk) depends on resistor values that are not
  legible in the schematic image — unverified.
* KEY3 and LED5 share GPIO19 (LED5: VDD3V3 → R76 → LED → IO19), so
  pressing KEY3 should light LED5 (schematic inference).
* KEY2/GPIO13 works only with DIP switch 1 ON.
* The piano's lowest/highest keys work as a remote control on this board
  too (`piano_core::controls`, see `doc/piano_coach_firmware.md`); C8
  (bookmark) does nothing on headless boards.

### 4.5 LEDs

* **Green LED4 on GPIO22**, used as the status LED: `early_init()`
  configures GPIO22 as output and sets it 1; `status_led(on, _)` writes
  `on` as the level. The headless loop shows it on while the piano is
  connected and blinking (400 ms) while searching.
* **Polarity discrepancy (open):** the board schematic wires LED4 as
  VDD3V3 → R14 → LED → IO22, i.e. lit when GPIO22 is **low**. The firmware
  treats high as "on". If the schematic matches the board, the LED is dark
  right after `early_init()` and while the piano is connected, and the
  searching blink is merely inverted. The
  observed behaviour on the bench unit was not recorded; the repo docs
  describe the intended behaviour ("on while connected").
* LED5 on GPIO19 (shared with KEY3), LED3 power, LED1/LED2 charger — not
  driven by the firmware.

### 4.6 microSD

* SDMMC host, slot 1 IO_MUX pins, **1-bit**: CLK GPIO14, CMD GPIO15, D0
  GPIO2. `SDMMC_FREQ_HIGHSPEED` (40 MHz), internal pull-ups flag on (the
  board also has pull-ups R31…R37), FAT mounted at `/sdcard`,
  `max_files = 5`, 16 KB allocation unit, never formats.
* Long UTF-8 file names (`FATFS_LFN_HEAP`, max 255,
  `FATFS_API_ENCODING_UTF_8`) — the library has Chinese names.
* Library root: `/sdcard/midi` or `/sdcard/MIDI`, else `/sdcard`; a root
  that holds one folder only (e.g. `JustPiano2-20160326/`) is stepped
  into (`browse.rs`). Bench: 2449 songs indexed in 11 s.
* 1-bit mode leaves DAT1 (GPIO4), DAT2 (GPIO12) and DAT3 (GPIO13, the
  KEY2 pin) free; card detect (GPIO34) is not configured
  (`SDMMC_SLOT_CONFIG_DEFAULT` has no CD pin). No card: the board still
  boots, logs "no microSD … insert one with a midi/ folder and press RST",
  and has no songs.
* DIP switch 3 must be ON (CMD). BOOT (K1) also pulls IO2 = DAT0 low;
  pressing it while the card is in use would disturb the bus.

### 4.7 Wi-Fi access point, captive portal, web page

From `src/web.rs`, `src/headless.rs`, `build.rs`:

* Access point **`PianoCoach-XXXX`** (XXXX = last two bytes of the softAP
  MAC in hex), WPA2-Personal, password **`pianocoach`**, channel 6, at
  most 4 clients. Address **192.168.4.1/24**, DHCP on, DNS server = the
  board.
* A tiny DNS server (UDP 53, thread with 4 KB PSRAM stack) answers every A
  query with 192.168.4.1; every unknown URL gets a 302 to
  `http://192.168.4.1/`, so phones show the "sign in to network" page.
* HTTP server: 10 KB task stack, 8 URI handlers, wildcard matching.
  `GET /` serves `web/index.html`, gzipped at build time by `build.rs`
  (≈ 11.6 KB → 3.5–4 KB). API: `/api/state`, `/api/dir?p=`,
  `/api/search?q=`, `/api/play?p=`, `/api/cmd?c=…&v=…` (playpause, stop,
  restart, next, prev, mode, hands, tempo, vol, sound, ledtest, alloff).
  Paths with `..` are refused.
* The HTTP server is created **before** Wi-Fi starts: Wi-Fi start-up
  fragments internal RAM and the server task needs a contiguous internal
  stack.
* Search index (pinyin) is built in a background thread (16 KB PSRAM
  stack) after boot.
* Bench: page 0.2–0.3 s, `/api/state` 0.1–0.4 s from a PC; captive portal
  from a phone ✔.
* Testing from the PC (AGENTS.md): join the AP with the dongle
  `wlxa0d768003aa5` (`nmcli dev wifi connect PianoCoach-XXXX password
  pianocoach ifname wlxa0d768003aa5`), set that profile `autoconnect no`
  and `ipv4.never-default yes`, and reconnect the home network afterwards.

### 4.8 BLE and radio sharing

* BLE only (`BTDM_CTRL_MODE_BLE_ONLY`), NimBLE central + observer, one
  connection, preferred MTU 260, bonds persisted in NVS, security off.
  Controller and NimBLE host on core 0; the UI and audio threads are
  pinned to core 1 (`threads::spawn_core1`, `audio.rs`).
* TX power raised to +9 dBm (the ESP32 default is +3 dBm) for default,
  scan and connection (`ble_midi.rs`, `cfg(esp32)`).
* **NimBLE host flow control OFF** (`CONFIG_BT_NIMBLE_HS_FLOW_CTRL=n`).
  ESP-IDF enables it for the original ESP32 only; with it the controller
  stops handing the piano's packets to NimBLE after as many as NimBLE has
  ACL buffers (`BT_NIMBLE_ACL_BUF_COUNT = 24`), the piano's key presses
  stop, the link drops (reason 0x208) and later connections get no ATT
  answers until a reset.
* **`ble_relaxed`** (set by `build.rs` for headless boards and every
  original ESP32): the connection is requested at 30–50 ms (24–40 × 1.25
  ms) with a **10 s** supervision timeout (`SUPERVISION_10MS = 1000`), and
  when the piano renegotiates its own 10 ms / 500 ms link the board asks
  again (at most twice per connection). Outgoing MIDI is gathered for
  15 ms per packet (`WRITE_GAP`) to leave airtime for Wi-Fi.
  AGENTS.md and `doc/piano_coach_firmware.md` say "4 s" — the code says
  10 s.
* Wi-Fi/BT coexistence: software coexistence enabled, no preference set
  (balanced). Preferring BT starved Wi-Fi (page loads > 8 s).
* The piano accepts one BLE central at a time: power off other boards and
  tablets when testing.

### 4.9 RAM budget

The original ESP32's ~520 KB internal SRAM is the limit with Wi-Fi + BLE +
HTTP + audio. What the firmware does about it:

* `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=0`: every plain malloc goes to
  PSRAM first (songs, the search index's thousands of small strings, lwIP
  buffers via `SPIRAM_TRY_ALLOCATE_WIFI_LWIP`); 32 KB internal reserved
  (`SPIRAM_MALLOC_RESERVE_INTERNAL=32768`) for DMA and drivers.
* Threads that never write flash keep their stacks in PSRAM
  (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y`, `threads::spawn_ext`):
  audio 12 KB, index 16 KB, dns 4 KB, tone 3 KB. On the ESP32 the cache —
  and with it PSRAM — is off during flash writes, so threads that write
  NVS (the UI thread, 10 KB, which saves prefs) keep internal stacks.
* Small Wi-Fi pools: 4 static RX, 16 dynamic RX, 16 dynamic TX buffers,
  no AMPDU.
* HTTP server started before Wi-Fi (see §4.7).
* Result: about 72 KB internal RAM free with everything running
  (`doc/piano_coach_firmware.md`). The headless UI logs internal free /
  largest block / PSRAM free "before Wi-Fi" and "after Wi-Fi".

## 5. Memory map, partitions, sdkconfig

`boards/audiokit/partitions.csv` (4 MB flash, songs on microSD — no song
pack partition, no OTA):

| Name | Type | SubType | Offset | Size |
|---|---|---|---|---|
| nvs | data | nvs | 0x9000 | 24 KB (0x6000) |
| phy_init | data | phy | 0xF000 | 4 KB |
| factory | app | factory | 0x10000 | 0x3F0000 (≈ 3.94 MB) |

App size at the port: 1.6 MB of 3.9 MB. Release builds of the
`piano-coach` package use `opt-level = "s"` (`CARGO_CONFIG` in
`board.env`).

NVS prefs (namespace `piano`, i32 unless noted): `mode`, `hands`, `tempo`,
`sound`, `spkvol`, `lastsong` (string). To set a pref from the PC, build an
NVS image of size 0x6000 and write it at 0x9000 (AGENTS.md) — this erases
all prefs and BLE bonds.

Notable `boards/audiokit/sdkconfig.defaults` settings:

| Setting | Why |
|---|---|
| `ESPTOOLPY_FLASHMODE_DIO`, `FLASHSIZE_4MB` | module flash |
| `ESP32_REV_MIN_3=y` | rev 3 silicon; PSRAM needs no cache workaround (rev 1 boards such as the WROVER-KIT cannot use PSRAM from Rust) |
| `ESP_DEFAULT_CPU_FREQ_MHZ_240` | synth + Wi-Fi + BLE |
| `FREERTOS_HZ=1000` | "MIDI timing needs ms resolution" |
| `ESP_MAIN_TASK_STACK_SIZE=8192`, `PTHREAD_TASK_STACK_SIZE_DEFAULT=8192` | a 4 KB main stack overflowed on Xtensa (found on the WROVER-KIT) |
| `ESP_TASK_WDT_TIMEOUT_S=15` | |
| `SPIRAM=y`, `SPIRAM_USE_MALLOC`, `SPIRAM_MALLOC_ALWAYSINTERNAL=0`, `SPIRAM_MALLOC_RESERVE_INTERNAL=32768`, `SPIRAM_TRY_ALLOCATE_WIFI_LWIP` | "every plain malloc goes there first (ALWAYSINTERNAL=0) — songs, the search index's thousands of small strings, lwIP buffers; internal RAM stays for Wi-Fi/BT/coexistence, DMA and the stacks of threads that write flash" |
| `SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y` | PSRAM thread stacks (`threads.rs`) |
| `FATFS_LFN_HEAP`, `FATFS_MAX_LFN=255`, `FATFS_API_ENCODING_UTF_8`, `FATFS_USE_LABEL` | "long file names in UTF-8 (the song library is Chinese)" |
| `ESP_COEX_SW_COEXIST_ENABLE` | Wi-Fi AP alongside BLE |
| `HTTPD_MAX_REQ_HDR_LEN=1024`, `HTTPD_MAX_URI_LEN=1024` | no comment in the file; evidently for long percent-encoded Chinese paths in `/api/play?p=` / `/api/dir?p=` URIs |
| `BT_ENABLED`, `BTDM_CTRL_MODE_BLE_ONLY`, `BTDM_CTRL_BLE_MAX_CONN=1`, `BT_NIMBLE_*` (central + observer, 1 connection, MTU 260, NVS persist, no security, host task stack 6144, log level warning) | "BLE only, NimBLE central, one link" |
| `BT_NIMBLE_HS_FLOW_CTRL=n` | "with it this controller stops handing the piano's packets to NimBLE once it has sent as many as NimBLE has ACL buffers (24 by default): the host's buffer credits evidently never come back" — see §4.8 |
| `ESP_WIFI_STATIC_RX_BUFFER_NUM=4`, `DYNAMIC_RX_BUFFER_NUM=16`, `DYNAMIC_TX_BUFFER_NUM=16`, `AMPDU_TX/RX_ENABLED=n` | "Internal RAM is the limit with Wi-Fi + BLE: small Wi-Fi buffer pools (a phone page needs little throughput)" |

The resolved build config also shows PSRAM quad mode at 40 MHz with
bank switching enabled (ESP-IDF defaults); the firmware does not use
himem.

## 6. Build, flash, console

```bash
cd firmware/piano-coach
tools/board.sh audiokit build --release
tools/board.sh audiokit run --release -- --port /dev/ttyUSB0   # flash + monitor (blocks)
```

* `tools/board.sh` reads `boards/audiokit/board.env`: `FEATURE=board-audiokit`,
  `MCU=esp32`, `ESP_IDF_VERSION=v5.5.3`, `TARGET=xtensa-esp32-espidf`,
  `TOOLCHAIN=esp198`, `FLASHER=espflash`, `UI=headless`,
  `HAS="sd speaker"`, `CARGO_CONFIG` = `opt-level "s"` for the app. It
  sets the extra-components file (`esp-idf-components.toml` → the C
  component `components/audiokit_bsp`, bindings module `bsp`), the
  sdkconfig defaults, `sdkconfig.audiokit`, the partition table and
  `CARGO_TARGET_DIR=target/audiokit`, and finds the esp-clang for bindgen.
  ESP-IDF tools are shared from
  `/home/nws/w/rust/herdline/clients/tab5/.embuild/espressif`.
* `build.rs` turns `UI=headless` / `HAS` into `cfg(ui_headless)`,
  `cfg(has_sd)`, `cfg(has_speaker)` and sets `cfg(ble_relaxed)`; for the
  headless UI it gzips `web/index.html` instead of compiling Slint.
* The first build compiles ESP-IDF (10+ minutes); run it in the
  background.
* Direct flashing:

  ```bash
  T=target/audiokit/xtensa-esp32-espidf/release
  espflash flash --chip esp32 --port /dev/ttyUSB0 --baud 921600 \
      --bootloader $T/bootloader.bin --partition-table $T/partition-table.bin $T/piano-coach
  ```

  Flashing the app keeps NVS (prefs, BLE bonds). No song pack is needed —
  songs come from the card.
* **Port identification**: the CP2102 shows up as `/dev/ttyUSB*`; other
  boards on the bench (Korvo CP2102N, CYD CH340, WROVER-KIT FT2232) also
  use `ttyUSB*`, so check with `udevadm info -q property -n /dev/ttyUSB0 |
  grep ID_SERIAL` or `espflash board-info --port /dev/ttyUSB0` (should
  report ESP32 rev 3.x, 4 MB flash) before flashing, and prefer
  `/dev/serial/by-id/…`.
* **Download mode**: automatic over DTR/RTS; manually hold BOOT (K1) and
  press RST (K2).
* **Console**: 115200 baud on the same port. Use pyserial with DTR and RTS
  false to read logs without resetting; pulse RTS to reset. Do not
  `pkill -f` a pattern that matches your own shell command; use PID files.
* Useful log lines: `board: ESP32-Audio-Kit`, `microSD mounted at
  /sdcard`, `audiokit: ES8388 reg 0x30 = 0x21` (and 0x31), `audio:
  speaker codec up (synth 24000 Hz → 24000 Hz × 2 ch)`, `headless: …
  internal RAM free …`, `web: Wi-Fi “PianoCoach-XXXX” …`, `headless:
  indexed N songs in …`, `ble-midi: link …: interval …`.

## 7. Piano Coach support

* **UI class**: headless (`headless.rs` + `web.rs` + `web/index.html`).
  Capabilities (`CAPS`): no backlight, no power latch, no USB-MIDI,
  BLE-MIDI yes, buttons yes, no touch.
* **Boot**: Follow mode with a random song from 儿时回忆; it starts as soon
  as the piano connects (or at once if notes go to the speaker). The last
  song played is remembered (`lastsong`).

Verified on hardware (all 2026-10-16; port commit dd08eb7, flow-control
fix a7c4765, verification b55369a):

| Check | Result |
|---|---|
| microSD library (2449 songs indexed in 11 s) | ✔ |
| ES8388 speaker, after raising LOUT2/ROUT2 | ✔ loud and clean |
| BLE to the piano | ✔ |
| Six keys with feedback beeps | ✔ |
| Web page from a phone (captive portal) and a PC (page 0.2–0.3 s, state 0.1–0.4 s) | ✔ |
| Two-way BLE (Follow, four songs started from the piano's control keys, flow control off): 3½ min, 542 notifications in, 1888 writes out, no drop or busy retry at a weak −86…−94 dBm | ✔ |
| Piano-key song keys | used during the BLE test (with the old folder stepping) |
| App size | 1.6 MB of 3.9 MB |

Not verified / not done on this board: the green LED's on/off sense
(§4.5); E1/F1 by category and shuffle-play's automatic next song (verified
on other boards only); bookmarks (C8 is a no-op on headless boards);
headphone output and detect; microphones and line-in; battery operation;
deep sleep / KEY2 wake (unreachable from the headless UI); KEY1 cross-talk
from the ladder (§4.4). The bench unit's antenna variant (PCB or IPEX)
was not recorded.

Known limits: no USB host (BLE only); no display, so the page or the keys
are the only UI; internal RAM leaves little headroom for new features
that need internal memory (DMA, flash-writing threads).

## 8. Quirks and lessons learned

1. **Speaker almost silent with esp_codec_dev defaults**: the driver sets
   LOUT2/ROUT2 (0x30/0x31) to −30 dB; write 0x21 to both through the
   control interface. `esp_codec_dev_write_reg()` does nothing for the
   ES8388 (no `set_reg` in the driver).
2. **Stereo output**: send two channels; each amplifier takes one.
3. **Host flow control must stay off** on this chip (§4.8) or BLE dies
   about a second into two-way playing.
4. **Internal RAM** runs out first: PSRAM-first malloc, PSRAM stacks for
   threads that never write flash, small Wi-Fi pools, HTTP server before
   Wi-Fi.
5. **Radio sharing**: do not prefer BT in coexistence; relax the BLE link
   to 30–50 ms / 10 s instead.
6. **DIP switch**: KEY2 and the SD card's CMD line both depend on it
   (switches 1 and 3 ON). JTAG costs KEY2 and the SD card.
7. **GPIO36 has no internal pull-up** and is an analog ladder node in the
   schematic; it works as KEY1 on the bench unit.
8. **MCLK must be GPIO0** (module wiring), which is also the boot strap —
   the module's 10 kΩ pull-up keeps it high at reset.
9. **ES8388 address 0x20 vs 0x10**: 8-bit vs 7-bit form of the same
   address.
10. **LED polarity**: code drives GPIO22 high for "on", the schematic says
    the LED lights on low — check on the bench before relying on it.
11. **Board schematic predates the ES8388 module**: for audio pins trust
    the V2.3 module spec and the replacement note over the board
    schematic image.

### Discrepancies and open questions

| # | Topic | Sources | Firmware follows |
|---|---|---|---|
| 1 | Green LED polarity | code: GPIO22 high = on; board schematic: LED4 cathode on IO22 (active low) | code; observed sense not recorded — open |
| 2 | Speaker amp inputs | code/bench: raising LOUT2/ROUT2 fixed the speaker; module spec: pins 23/25 NC, 24/26 = ROUT1/LOUT1, 27/28 = LOUT2/ROUT2; replacement note: + = HPOUTL/R (LOUT2/ROUT2), − = SPOLN/SPORN (LOUT1/ROUT1); board schematic (AC101 era): amp + inputs on SPOLP/SPORP | code; actual wiring on ES8388 boards not shown — open |
| 3 | BLE supervision timeout | code: 10 s; AGENTS.md and firmware doc: 4 s | code (repo docs stale) |
| 4 | Key pull-ups | board spec: "no external pull-up"; board schematic: R52/R54 + ladder to VDD3V3 | internal pull-ups on 13/19/23/18/5; GPIO36 relies on the external chain |
| 5 | GPIO36 / KEY1 | board schematic: analog `KEY_AD` ladder node for all keys; code: digital read | code; cross-talk unverified |
| 6 | MCLK pin reason | BSP comment: ESP32 can only use GPIO0; ADF: GPIO0/1/3 possible | GPIO0 (module wiring) |
| 7 | Module pin names 23/25, 27/28 | board schematic: SPORP/SPOLP, HPOUTL/R; module schematic: 27/28 labelled LINEINL/LINEINR; module spec: 23/25 NC, 27/28 HPOUTL/R = LOUT2/ROUT2 | module spec |
| 8 | ES8388 I²C address | BSP: 0x20; driver shifts to 7-bit 0x10 | both correct |
| 9 | Part numbers U1 (charger), U2 (buck), U4/U5 (amps), MIC3/MIC4, resistor values | not legible in the schematic image; "NS4150B" only from `doc/piano_coach_firmware.md` | — unknown |
| 10 | GPIO12 strap vs SD DAT2 pull-up | board schematic pull-up on DAT2; ESP32 MTDI strap | board boots; mechanism unknown |

## 9. Sources

In the repo:

* `doc/boards/vendor/audiokit/esp32-a1s-es8388-module-v2.3-schematic.pdf`
  — ESP32-A1S (ES8388) module V2.3 schematic (2 pages; ES8388 sheet dated
  2021-06-02).
* `doc/boards/vendor/audiokit/audio-kit-v2.2-spec-cn.pdf` — ESP32-Audio-Kit
  V2.2 board spec V1.1 (Chinese): parameters, header pinout (table 8), DIP
  switch (table 9), keys (table 10), board schematic image (page 15, dated
  2019-04-09).
* `doc/boards/vendor/audiokit/esp32-a1s-es8388-module-v2.3-spec-en.pdf` —
  ESP32-A1S (ES8388) module spec V2.3 (English): pin table 8, boot modes
  table 9.
* `firmware/piano-coach/boards/audiokit/` — `board.env`,
  `sdkconfig.defaults`, `partitions.csv`, `esp-idf-components.toml`.
* `firmware/piano-coach/src/board/audiokit/mod.rs`, `src/board/key.rs`,
  `src/board/mod.rs`, `src/board/contract.rs`.
* `firmware/piano-coach/components/audiokit_bsp/` — `audiokit_bsp.c`,
  `include/audiokit_bsp.h`, `CMakeLists.txt`, `idf_component.yml`.
* `firmware/piano-coach/src/headless.rs`, `src/web.rs`, `src/threads.rs`,
  `src/audio.rs`, `src/ble_midi.rs`, `src/main.rs`, `src/browse.rs`,
  `build.rs`, `tools/board.sh`.
* esp_codec_dev 1.5.11 as fetched into
  `target/audiokit/…/managed_components/espressif__esp_codec_dev`
  (`device/es8388/es8388.c`, `platform/audio_codec_ctrl_i2c.c`,
  `platform/audio_codec_data_i2s.c`) — driver register writes, volume,
  address handling and I²S reconfiguration on open.
* `AGENTS.md`, `doc/piano_coach_firmware.md` (headless section,
  architecture bullets, verified paragraphs), `doc/boards/README.md`,
  `firmware/piano-coach/boards/README.md`; git history dd08eb7, a7c4765,
  b55369a.

Outside the repo (`/home/nws/w/esp32/doc/ESP32-Audio-Kit/`):

* `驱动包更新/audio_board/esp32_audio_kit_v2_3/` (`board_def.h`,
  `board_pins_config.c`, `board.c`) — Ai-Thinker's ESP-ADF board package:
  key, LED, PA, headphone/aux detect, SD detect, I²C and I²S pins;
  `驱动包更新/readme.txt` (V2.2 A404 and V2.3 pin definitions may have
  changed, use this package).
* `3-ESP32-A1S (ES8388版) V2.3 版本替换 ESP32-A1S (AC101版) 说明/…说明.pdf` —
  ES8388 V2.3 replacing AC101: output pin pairs, line-in/MIC2 sharing.
* `ESP32-Audio-Kit 音频开发板说明.docx` — overview; AC101 modules
  discontinued, boards sold since 2021 use the ES8388 module.
* `参考资料链接.txt` — vendor page
  https://vdoc.ai-thinker.com/%E9%9F%B3%E9%A2%91%E5%BC%80%E5%8F%91%E6%9D%BF_esp32-audio-kit
  and https://vdoc.ai-thinker.com/.
