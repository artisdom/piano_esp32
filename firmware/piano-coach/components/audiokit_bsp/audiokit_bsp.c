// Ai-Thinker ESP32-Audio-Kit: ES8388 speaker codec and microSD, following
// the pin map of the vendor's ESP-ADF board package (esp32_audio_kit_v2_3)
// and the Tab5 BSP's ES8388 bring-up.
#include "audiokit_bsp.h"

#include "driver/i2c_master.h"
#include "driver/i2s_std.h"
#include "driver/sdmmc_host.h"
#include "esp_codec_dev_defaults.h"
#include "esp_log.h"
#include "esp_vfs_fat.h"
#include "sdmmc_cmd.h"

static const char *TAG = "audiokit";

esp_codec_dev_handle_t audiokit_speaker_init(void)
{
    i2c_master_bus_config_t bus_cfg = {
        .i2c_port = 0,
        .sda_io_num = 33,
        .scl_io_num = 32,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    i2c_master_bus_handle_t bus = NULL;
    if (i2c_new_master_bus(&bus_cfg, &bus) != ESP_OK) {
        ESP_LOGE(TAG, "I2C bus init failed");
        return NULL;
    }

    i2s_chan_handle_t tx = NULL;
    i2s_chan_config_t chan_cfg = I2S_CHANNEL_DEFAULT_CONFIG(I2S_NUM_0, I2S_ROLE_MASTER);
    chan_cfg.auto_clear = true;
    if (i2s_new_channel(&chan_cfg, &tx, NULL) != ESP_OK) {
        ESP_LOGE(TAG, "I2S channel failed");
        return NULL;
    }
    i2s_std_config_t std_cfg = {
        .clk_cfg = I2S_STD_CLK_DEFAULT_CONFIG(48000),
        .slot_cfg = I2S_STD_PHILIP_SLOT_DEFAULT_CONFIG(I2S_DATA_BIT_WIDTH_16BIT, I2S_SLOT_MODE_MONO),
        .gpio_cfg = {
            // The ESP32 can put MCLK out on GPIO0 only (CLK_OUT1).
            .mclk = GPIO_NUM_0,
            .bclk = GPIO_NUM_27,
            .ws = GPIO_NUM_25,
            .dout = GPIO_NUM_26,
            .din = I2S_GPIO_UNUSED,
        },
    };
    if (i2s_channel_init_std_mode(tx, &std_cfg) != ESP_OK || i2s_channel_enable(tx) != ESP_OK) {
        ESP_LOGE(TAG, "I2S init failed");
        return NULL;
    }
    audio_codec_i2s_cfg_t i2s_cfg = {.port = I2S_NUM_0, .tx_handle = tx, .rx_handle = NULL};
    const audio_codec_data_if_t *data_if = audio_codec_new_i2s_data(&i2s_cfg);

    audio_codec_i2c_cfg_t i2c_cfg = {.port = 0, .addr = ES8388_CODEC_DEFAULT_ADDR, .bus_handle = bus};
    const audio_codec_ctrl_if_t *ctrl_if = audio_codec_new_i2c_ctrl(&i2c_cfg);
    const audio_codec_gpio_if_t *gpio_if = audio_codec_new_gpio();
    if (!data_if || !ctrl_if || !gpio_if) {
        ESP_LOGE(TAG, "codec interfaces failed");
        return NULL;
    }
    es8388_codec_cfg_t es_cfg = {
        .ctrl_if = ctrl_if,
        .gpio_if = gpio_if,
        .codec_mode = ESP_CODEC_DEV_WORK_MODE_DAC,
        .pa_pin = 21,
        .pa_reverted = false,
        .master_mode = false,
        .hw_gain = {.pa_voltage = 5.0, .codec_dac_voltage = 3.3},
    };
    const audio_codec_if_t *codec = es8388_codec_new(&es_cfg);
    if (!codec) {
        ESP_LOGE(TAG, "ES8388 not found");
        return NULL;
    }
    esp_codec_dev_cfg_t dev_cfg = {.dev_type = ESP_CODEC_DEV_TYPE_OUT, .codec_if = codec, .data_if = data_if};
    esp_codec_dev_handle_t dev = esp_codec_dev_new(&dev_cfg);
    if (dev) {
        // The speaker amplifiers hang on LOUT2/ROUT2, which the driver
        // leaves at -30 dB (only the headphone outputs get 0 dB): +3 dB, the
        // maximum. Over the control interface: the ES8388 driver has no
        // set_reg, so esp_codec_dev_write_reg() would do nothing. The driver
        // writes these registers only when the codec is created, so once is
        // enough.
        for (int reg = 0x30; reg <= 0x31; reg++) { // DACCONTROL26/27
            int v = 0x21;
            ctrl_if->write_reg(ctrl_if, reg, 1, &v, 1);
            v = -1;
            ctrl_if->read_reg(ctrl_if, reg, 1, &v, 1);
            ESP_LOGI(TAG, "ES8388 reg 0x%02x = 0x%02x", reg, v & 0xff);
        }
    }
    return dev;
}

esp_err_t audiokit_sdcard_mount(void)
{
    sdmmc_host_t host = SDMMC_HOST_DEFAULT();
    host.max_freq_khz = SDMMC_FREQ_HIGHSPEED;
    sdmmc_slot_config_t slot = SDMMC_SLOT_CONFIG_DEFAULT();
    slot.width = 1;
    slot.flags |= SDMMC_SLOT_FLAG_INTERNAL_PULLUP;
    esp_vfs_fat_sdmmc_mount_config_t mount_cfg = {
        .format_if_mount_failed = false,
        .max_files = 5,
        .allocation_unit_size = 16 * 1024,
    };
    sdmmc_card_t *card = NULL;
    esp_err_t err = esp_vfs_fat_sdmmc_mount("/sdcard", &host, &slot, &mount_cfg, &card);
    if (err == ESP_OK) {
        sdmmc_card_print_info(stdout, card);
    }
    return err;
}
