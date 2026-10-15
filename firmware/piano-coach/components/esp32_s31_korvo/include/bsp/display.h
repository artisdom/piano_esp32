/*
 * SPDX-FileCopyrightText: 2026 Espressif Systems (Shanghai) CO LTD
 *
 * SPDX-License-Identifier: Apache-2.0
 */

/**
 * @file
 * @brief ESP32-S31-Korvo BSP display interface
 */

#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_touch.h"

/*
 * Herdline fork: upstream registers this panel with esp_lvgl_adapter and
 * exposes lv_display_t/lv_indev_t here. Herdline renders with Slint, and the
 * component is marked WHOLE_ARCHIVE, so keeping those entry points would link
 * the whole of LVGL into an image that never calls it. The LVGL surface
 * (bsp_display_start[_with_config], bsp_display_get, bsp_display_get_input_dev,
 * bsp_display_lock/unlock) and the esp_lvgl_adapter dependency are removed;
 * the panel timings below match the verified SUB3 ST7262 panel. Rotation, which
 * only ever fed the adapter and the touch coordinate mapping, becomes a local
 * enum with the same values.
 */

#ifdef __cplusplus
extern "C" {
#endif

/** @defgroup display Display
 *  @brief LCD and LVGL BSP API
 *  @{
 */

#define BSP_LCD_H_RES                 (800)
#define BSP_LCD_V_RES                 (480)
/* SUB_BOARD3_800_480_PANEL_35HZ_RGB_TIMING, verified on the Korvo demo.
 * The upstream Korvo 26 MHz / one-line VSYNC timings roll on this panel. */
#define BSP_LCD_PIXEL_CLOCK_HZ        (18 * 1000 * 1000)
#define BSP_LCD_DATA_WIDTH            (16)
#define BSP_LCD_BITS_PER_PIXEL        (16)

#define BSP_LCD_HSYNC_PULSE_WIDTH     (40)
#define BSP_LCD_HSYNC_BACK_PORCH      (40)
#define BSP_LCD_HSYNC_FRONT_PORCH     (48)
#define BSP_LCD_VSYNC_PULSE_WIDTH     (23)
#define BSP_LCD_VSYNC_BACK_PORCH      (32)
#define BSP_LCD_VSYNC_FRONT_PORCH     (13)

/**
 * @brief Panel mounting rotation, used for touch coordinate mapping.
 *
 * Values match esp_lv_adapter_rotation_t so a caller that wants the upstream
 * LVGL path back can pass either.
 */
typedef enum {
    BSP_DISPLAY_ROTATE_0 = 0,
    BSP_DISPLAY_ROTATE_90,
    BSP_DISPLAY_ROTATE_180,
    BSP_DISPLAY_ROTATE_270,
} bsp_display_rotation_t;

#define BSP_LCD_ROTATION_DEFAULT      BSP_DISPLAY_ROTATE_0

/**
 * @brief BSP display configuration
 *
 * Upstream's LVGL-only fields (tear avoidance, draw-buffer height, adapter
 * task stack, PPA acceleration, touch registration) are gone with the adapter.
 */
typedef struct {
    bsp_display_rotation_t rotation;  /*!< Panel mounting rotation */
} bsp_display_config_t;

#define BSP_DISPLAY_DEFAULT_CONFIG() {                                      \
    .rotation        = BSP_LCD_ROTATION_DEFAULT,                            \
}

/**
 * @brief Create and initialize the RGB LCD panel.
 *
 * @param[in] config Display configuration. NULL selects BSP_DISPLAY_DEFAULT_CONFIG().
 * @param[out] ret_panel Returned RGB panel handle.
 *
 * @return ESP_OK on success, otherwise an ESP-IDF error code.
 */
esp_err_t bsp_display_new(const bsp_display_config_t *config, esp_lcd_panel_handle_t *ret_panel);

/**
 * @brief Get the RGB LCD panel handle created by the BSP.
 */
esp_lcd_panel_handle_t bsp_display_get_panel(void);

/** Publish dirty rows and wait until GDMA releases the previous framebuffer. */
esp_err_t bsp_display_present(int y_start, int y_end, const void *framebuffer);

/**
 * @brief Create the GT1151 touch controller used by the RGB 800x480 panel.
 *
 * @param[in] rotation Display rotation used for touch coordinate mapping.
 * @param[out] ret_touch Returned touch handle.
 *
 * @return ESP_OK on success, otherwise an ESP-IDF error code.
 */
esp_err_t bsp_touch_new(bsp_display_rotation_t rotation, esp_lcd_touch_handle_t *ret_touch);

/**
 * @brief Turn on LCD backlight if the control GPIO is available.
 */
esp_err_t bsp_display_backlight_on(void);

/**
 * @brief Turn off LCD backlight if the control GPIO is available.
 */
esp_err_t bsp_display_backlight_off(void);

/** @} */

#ifdef __cplusplus
}
#endif
