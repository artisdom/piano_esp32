/*
 * SPDX-FileCopyrightText: 2026 Espressif Systems (Shanghai) CO LTD
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "esp_log.h"
#include "bsp_err_check.h"
#include "usb/usb_host.h"

#include "bsp/m5stack_tab5.h"

static const char *TAG = "M5Stack Tab5";
static TaskHandle_t usb_host_task;  // USB Host Library task

static void usb_lib_task(void *arg)
{
    while (1) {
        // Start handling system events
        uint32_t event_flags;
        usb_host_lib_handle_events(portMAX_DELAY, &event_flags);
        if (event_flags & USB_HOST_LIB_EVENT_FLAGS_NO_CLIENTS) {
            /* VENDORED PATCH (herdline): during teardown this can race the
             * host uninstall and return an error — log it, never abort
             * (logs/tab5/64 crashed here). */
            esp_err_t free_err = usb_host_device_free_all();
            if (free_err != ESP_OK) {
                ESP_LOGW(TAG, "usb_host_device_free_all: %s", esp_err_to_name(free_err));
            }
        }
        if (event_flags & USB_HOST_LIB_EVENT_FLAGS_ALL_FREE) {
            ESP_LOGI(TAG, "USB: All devices freed");
            // Continue handling USB events to allow device reconnection
            // The only way this task can be stopped is by calling bsp_usb_host_stop()
        }
    }
}

esp_err_t bsp_usb_host_start(bsp_usb_host_power_mode_t mode, bool limit_500mA)
{
    /* Enable Feature */
    BSP_ERROR_CHECK_RETURN_ERR(bsp_feature_enable(BSP_FEATURE_USB, true));

    //Install USB Host driver. Should only be called once in entire application
    ESP_LOGI(TAG, "Installing USB Host");
    const usb_host_config_t host_config = {
        .skip_phy_setup = false,
        .intr_flags = ESP_INTR_FLAG_LOWMED,
    };
    /* VENDORED PATCH (herdline): soft-fail instead of the aborting
     * BSP_ERROR_CHECK — an install failure (ESP_ERR_NO_MEM under internal-RAM
     * pressure, logs/tab5/63) must surface to the caller, not reboot a crush
     * terminal mid-yard. Drop the 5 V rail again on the way out. */
    esp_err_t install_err = usb_host_install(&host_config);
    if (install_err != ESP_OK) {
        ESP_LOGE(TAG, "usb_host_install failed: %s", esp_err_to_name(install_err));
        bsp_feature_enable(BSP_FEATURE_USB, false);
        return install_err;
    }

    // Create a task that will handle USB library events
    if (xTaskCreate(usb_lib_task, "usb_lib", 4096, NULL, 10, &usb_host_task) != pdTRUE) {
        ESP_LOGE(TAG, "Creating USB host lib task failed");
        abort();
    }

    return ESP_OK;
}

esp_err_t bsp_usb_host_stop(void)
{
    /* VENDORED PATCH (herdline): stop the event task BEFORE uninstalling —
     * the original order let usb_lib_task wake mid-uninstall and abort on
     * usb_host_device_free_all (logs/tab5/64). */
    if (usb_host_task) {
        vTaskSuspend(usb_host_task);
        vTaskDelete(usb_host_task);
        usb_host_task = NULL;
    }
    usb_host_uninstall();
    return ESP_OK;
}
