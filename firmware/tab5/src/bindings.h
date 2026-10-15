// Extra bindgen input: the Tab5 BSP plus the USB host library (for the
// USB-MIDI class client in usb_midi.rs).
#include "bsp/m5stack_tab5.h"
#include "bsp/display.h"
#include "bsp/touch.h"
#include "esp_lcd_mipi_dsi.h"
#include "esp_lcd_touch.h"
#include "esp_lcd_panel_ops.h"
#include "esp_cache.h"
#include "esp_vfs_fat.h"
#include "usb/usb_host.h"
#include "usb/usb_helpers.h"

// NimBLE host (esp-idf-sys's own bindings exclude BT on the ESP32-P4, which
// predates hosted BT) and the ESP-Hosted controller RPCs.
#ifdef CONFIG_BT_NIMBLE_ENABLED
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#include "services/gap/ble_svc_gap.h"
#include "esp_hosted.h"
#include "esp_hosted_misc.h"
#endif
