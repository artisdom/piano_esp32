// VCC-GND YD-ESP32-S3: USB OTG host and WS2812 (see include/yds3_bsp.h).

#include "yds3_bsp.h"

#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "led_strip.h"
#include "usb/usb_host.h"

static const char *TAG = "yds3";

static void usb_lib_task(void *arg)
{
    for (;;) {
        uint32_t flags = 0;
        usb_host_lib_handle_events(portMAX_DELAY, &flags);
        if (flags & USB_HOST_LIB_EVENT_FLAGS_NO_CLIENTS) {
            usb_host_device_free_all();
        }
    }
}

esp_err_t yds3_usb_host_start(void)
{
    const usb_host_config_t cfg = {
        .skip_phy_setup = false,
        .intr_flags = ESP_INTR_FLAG_LEVEL1,
    };
    esp_err_t err = usb_host_install(&cfg);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "usb_host_install: %s", esp_err_to_name(err));
        return err;
    }
    if (xTaskCreatePinnedToCore(usb_lib_task, "usb-lib", 4096, NULL, 10, NULL, 0) != pdPASS) {
        return ESP_ERR_NO_MEM;
    }
    ESP_LOGI(TAG, "USB host up on the OTG port");
    return ESP_OK;
}

static led_strip_handle_t s_led;

esp_err_t yds3_led_init(void)
{
    const led_strip_config_t strip = {
        .strip_gpio_num = 48,
        .max_leds = 1,
        .led_model = LED_MODEL_WS2812,
        .color_component_format = LED_STRIP_COLOR_COMPONENT_FMT_GRB,
    };
    const led_strip_rmt_config_t rmt = {
        .resolution_hz = 10 * 1000 * 1000,
    };
    esp_err_t err = led_strip_new_rmt_device(&strip, &rmt, &s_led);
    if (err == ESP_OK) {
        led_strip_clear(s_led);
    }
    return err;
}

void yds3_led_set(uint8_t r, uint8_t g, uint8_t b)
{
    if (!s_led) {
        return;
    }
    led_strip_set_pixel(s_led, 0, r, g, b);
    led_strip_refresh(s_led);
}
