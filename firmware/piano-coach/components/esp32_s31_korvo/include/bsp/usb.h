#pragma once
#include "esp_err.h"
#ifdef __cplusplus
extern "C" {
#endif
/** Install the native Type-A USB HS host and its library event task. */
esp_err_t bsp_usb_host_start(void);
/** Stop after class drivers release their clients; bounded to five seconds. */
esp_err_t bsp_usb_host_stop(void);
#ifdef __cplusplus
}
#endif
