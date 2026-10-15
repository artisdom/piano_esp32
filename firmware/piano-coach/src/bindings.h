// Extra bindgen input: the selected board's BSP plus common ESP-IDF APIs,
// exposed to Rust as esp_idf_svc::sys::bsp. Each board adds a branch keyed
// on its target; the common parts below are used by src/board/mod.rs,
// usb_midi.rs, ble_midi.rs, audio.rs and songs.rs.
#include "sdkconfig.h"

#if CONFIG_IDF_TARGET_ESP32P4
// M5Stack Tab5
# define PIANO_UI_FULL 1
# include "bsp/m5stack_tab5.h"
# include "bsp/display.h"
# include "bsp/touch.h"
# include "esp_lcd_mipi_dsi.h"
#elif CONFIG_IDF_TARGET_ESP32S31
// ESP32-S31-Korvo-1
# define PIANO_UI_FULL 1
# include "bsp/esp32_s31_korvo.h"
# include "bsp/display.h"
# include "bsp/usb.h"
# include "bsp/button.h"
# include "bsp/audio.h"
# include "bsp/sdcard.h"
# include "esp_lcd_panel_rgb.h"
# include "iot_button.h"
#elif CONFIG_IDF_TARGET_ESP32S3
// ESP32-1732S019 (ST7789 on SPI) and LILYGO T-Display-S3 (ST7789 on an
// 8-bit i80 bus, CST816 touch on I2C): drivers in ESP-IDF.
# include "esp_lcd_panel_vendor.h"
# include "esp_lcd_io_i80.h"
# include "driver/i2c_master.h"
#elif CONFIG_IDF_TARGET_ESP32
// ESP-WROVER-KIT: ILI9341 / ST7789V on SPI, driven by esp_lcd's ST7789 driver.
# include "esp_lcd_panel_vendor.h"
# include "driver/i2c_master.h" // board/cst816.rs is built for all small boards
#elif CONFIG_IDF_TARGET_ESP32C3
// ESP32-2424S012: GC9A01 on SPI (managed component), CST816D on I2C.
# include "esp_lcd_gc9a01.h"
# include "driver/i2c_master.h"
#else
# error "Piano Coach: unsupported board target — add it to src/bindings.h"
#endif

#include "esp_lcd_panel_ops.h"
#include "esp_partition.h"
#include "esp_sleep.h"

#if PIANO_UI_FULL
// Framebuffer boards with touch, microSD, codec and USB host.
# include "esp_lcd_touch.h"
# include "esp_cache.h"
# include "esp_vfs_fat.h"
# include "esp_codec_dev.h"
# include "usb/usb_host.h"
# include "usb/usb_helpers.h"
#else
// Small SPI-panel boards.
# include "esp_lcd_panel_io.h"
# include "driver/spi_master.h"
# include "driver/gpio.h"
# include "driver/ledc.h"
#endif

// NimBLE host (esp-idf-sys's own bindings exclude BT on some targets).
#ifdef CONFIG_BT_NIMBLE_ENABLED
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#endif

// Coprocessor BLE (Tab5: ESP32-C6 over SDIO).
#if CONFIG_IDF_TARGET_ESP32P4
# include "esp_hosted.h"
# include "esp_hosted_misc.h"
#endif
