/* Korvo's Type-A port has an always-powered TPS2051C 500 mA VBUS switch.
 * The native PHY needs no GPIO muxing or Tab5 expander/power sequence. */
#include <stdatomic.h>
#include "bsp/usb.h"
#include "esp_intr_alloc.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "usb/usb_host.h"

static const char *TAG = "S31-USB";
static bool s_installed;
static atomic_bool s_stopping;
static SemaphoreHandle_t s_stopped;
static esp_err_t s_stop_result;

static void usb_events(void *arg)
{
    // Installation allocates the IRQ on this core. Keep install, event
    // handling and uninstall on the same core, matching the IDF MSC example.
    // Cross-core teardown can race a pending root-port IRQ with hub deletion.
    const usb_host_config_t config = {
        .skip_phy_setup = false,
        .intr_flags = ESP_INTR_FLAG_LOWMED,
    };
    s_stop_result = usb_host_install(&config);
    xSemaphoreGive(s_stopped);
    if (s_stop_result != ESP_OK) {
        vTaskDelete(NULL);
        return;
    }
    while (true) {
        uint32_t flags = 0;
        esp_err_t ret = usb_host_lib_handle_events(pdMS_TO_TICKS(50), &flags);
        if (ret != ESP_OK && ret != ESP_ERR_TIMEOUT) {
            ESP_LOGW(TAG, "host events: %s", esp_err_to_name(ret));
        }
        if (atomic_load(&s_stopping)) {
            // MSC must deregister first. Free outstanding device objects,
            // servicing library events until all transfers have completed.
            if (usb_host_device_free_all() == ESP_OK) {
                s_stop_result = usb_host_uninstall();
                if (s_stop_result == ESP_OK) {
                    xSemaphoreGive(s_stopped);
                    vTaskDelete(NULL);
                    return;
                }
                atomic_store(&s_stopping, false);
                xSemaphoreGive(s_stopped);
            }
        }
    }
}

esp_err_t bsp_usb_host_start(void)
{
    if (s_installed) {
        return ESP_ERR_INVALID_STATE;
    }
    s_stopped = xSemaphoreCreateBinary();
    if (!s_stopped) {
        return ESP_ERR_NO_MEM;
    }
    atomic_store(&s_stopping, false);
    if (xTaskCreatePinnedToCore(usb_events, "s31_usb", 4096, NULL, 7, NULL, 1) != pdTRUE) {
        vSemaphoreDelete(s_stopped);
        s_stopped = NULL;
        return ESP_ERR_NO_MEM;
    }
    xSemaphoreTake(s_stopped, portMAX_DELAY);
    if (s_stop_result != ESP_OK) {
        vSemaphoreDelete(s_stopped);
        s_stopped = NULL;
        return s_stop_result;
    }
    s_installed = true;
    ESP_LOGI(TAG, "native Type-A USB HS host installed");
    return ESP_OK;
}

esp_err_t bsp_usb_host_stop(void)
{
    if (!s_installed) {
        return ESP_OK;
    }
    atomic_store(&s_stopping, true);
    usb_host_lib_unblock();
    if (xSemaphoreTake(s_stopped, pdMS_TO_TICKS(5000)) != pdTRUE) {
        return ESP_ERR_TIMEOUT;
    }
    if (s_stop_result == ESP_OK) {
        s_installed = false;
        vSemaphoreDelete(s_stopped);
        s_stopped = NULL;
        ESP_LOGI(TAG, "USB host uninstalled");
    }
    return s_stop_result;
}
