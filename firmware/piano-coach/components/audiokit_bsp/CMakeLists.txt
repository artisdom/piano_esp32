idf_component_register(
    SRCS "audiokit_bsp.c"
    INCLUDE_DIRS "include"
    REQUIRES esp_driver_i2s esp_driver_i2c esp_driver_sdmmc fatfs sdmmc esp_driver_gpio
)
