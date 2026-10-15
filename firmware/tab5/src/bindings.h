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
