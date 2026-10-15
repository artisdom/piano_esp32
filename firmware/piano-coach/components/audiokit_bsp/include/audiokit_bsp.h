// Ai-Thinker ESP32-Audio-Kit V2.2/V2.3 (ESP32-A1S module with ES8388):
// speaker codec and microSD bring-up for Piano Coach (src/board/audiokit).
#pragma once
#include "esp_err.h"
#include "esp_codec_dev.h"

#ifdef __cplusplus
extern "C" {
#endif

// ES8388 on I2C (SDA 33, SCL 32) and I2S0 (MCLK GPIO0, BCK 27, WS 25,
// DOUT 26), power amplifier enable on GPIO21. NULL on failure.
esp_codec_dev_handle_t audiokit_speaker_init(void);

// microSD in 1-bit SDMMC mode (CLK 14, CMD 15, D0 2), FAT mounted at
// /sdcard.
esp_err_t audiokit_sdcard_mount(void);

#ifdef __cplusplus
}
#endif
