//! Thread spawning with the stack in PSRAM where the board allows it
//! (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY`, the ESP32-Audio-Kit): its
//! internal RAM is needed for Wi-Fi, Bluetooth and DMA. Only for threads
//! that never write to flash (NVS) — on the ESP32 the cache, and with it
//! PSRAM, is off during flash writes. Elsewhere a plain spawn.

use std::thread::{Builder, JoinHandle};

pub fn spawn_ext<F>(name: &str, stack: usize, f: F) -> std::io::Result<JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    #[cfg(esp_idf_spiram_allow_stack_external_memory)]
    unsafe {
        use esp_idf_svc::sys as sys;
        let mut saved = sys::esp_pthread_get_default_config();
        let had = sys::esp_pthread_get_cfg(&mut saved) == 0;
        let mut cfg = if had { saved } else { sys::esp_pthread_get_default_config() };
        cfg.stack_alloc_caps = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
        cfg.inherit_cfg = false;
        let _ = sys::esp_pthread_set_cfg(&cfg);
        let r = Builder::new().name(name.into()).stack_size(stack).spawn(f);
        let restore = if had { saved } else { sys::esp_pthread_get_default_config() };
        let _ = sys::esp_pthread_set_cfg(&restore);
        r
    }
    #[cfg(not(esp_idf_spiram_allow_stack_external_memory))]
    Builder::new().name(name.into()).stack_size(stack).spawn(f)
}

/// Spawn on core 1 (pinned). On the original ESP32 the Bluetooth controller
/// and the NimBLE host live on core 0; a busy UI there (and the LCD's SPI
/// interrupts, which follow the core that opened the panel) starved the
/// link until the piano dropped it (ESP32-2432S028). Elsewhere a plain
/// spawn.
pub fn spawn_core1<F>(name: &str, stack: usize, f: F) -> std::io::Result<JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    #[cfg(esp32)]
    unsafe {
        use esp_idf_svc::sys as sys;
        let mut saved = sys::esp_pthread_get_default_config();
        let had = sys::esp_pthread_get_cfg(&mut saved) == 0;
        let mut cfg = if had { saved } else { sys::esp_pthread_get_default_config() };
        cfg.pin_to_core = 1;
        cfg.inherit_cfg = false;
        let _ = sys::esp_pthread_set_cfg(&cfg);
        let r = Builder::new().name(name.into()).stack_size(stack).spawn(f);
        let restore = if had { saved } else { sys::esp_pthread_get_default_config() };
        let _ = sys::esp_pthread_set_cfg(&restore);
        r
    }
    #[cfg(not(esp32))]
    Builder::new().name(name.into()).stack_size(stack).spawn(f)
}
