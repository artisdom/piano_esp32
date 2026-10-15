// Extra bindgen input: the selected board's BSP plus common ESP-IDF APIs,
// exposed to Rust as esp_idf_svc::sys::bsp. Each board adds a branch keyed
// on its target; the common part below is used by src/board/mod.rs,
// usb_midi.rs, ble_midi.rs and audio.rs.
#include "sdkconfig.h"

#if CONFIG_IDF_TARGET_ESP32P4
// M5Stack Tab5
# include "bsp/m5stack_tab5.h"
# include "bsp/display.h"
# include "bsp/touch.h"
# include "esp_lcd_mipi_dsi.h"
#elif CONFIG_IDF_TARGET_ESP32S31
// ESP32-S31-Korvo-1
# include "bsp/esp32_s31_korvo.h"
# include "bsp/display.h"
# include "bsp/usb.h"
# include "bsp/button.h"
# include "bsp/audio.h"
# include "bsp/sdcard.h"
# include "esp_lcd_panel_rgb.h"
# include "iot_button.h"
#else
# error "Piano Coach: unsupported board target — add it to src/bindings.h"
#endif

#include "esp_lcd_touch.h"
#include "esp_lcd_panel_ops.h"
#include "esp_cache.h"
#include "esp_vfs_fat.h"
#include "esp_codec_dev.h"
#include "usb/usb_host.h"
#include "usb/usb_helpers.h"

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
