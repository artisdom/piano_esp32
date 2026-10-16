// VCC-GND Studio YD-ESP32-S3 (ESP32-S3-WROOM-1 N16R8): USB OTG host on the
// native USB port and the WS2812 RGB LED, for Piano Coach (src/board/yds3).
#pragma once
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

// Install the USB host library on the S3's full-speed OTG port (GPIO19/20)
// and run its event loop on a task of its own. The port's VBUS comes from
// the board's 5 V through the bridged "USB-OTG" jumper.
esp_err_t yds3_usb_host_start(void);

// The WS2812 on GPIO48 (RMT); colour 0..255 per channel.
esp_err_t yds3_led_init(void);
void yds3_led_set(uint8_t r, uint8_t g, uint8_t b);

#ifdef __cplusplus
}
#endif
