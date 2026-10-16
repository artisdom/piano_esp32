//! Ai-Thinker ESP32-Audio-Kit V2.2/V2.3 — ESP32-A1S module (ESP32 rev 3,
//! 4 MB flash, 8 MB PSRAM of which 4 MB mapped, ES8388 codec), speaker
//! amplifier and headphone jack, microSD (1-bit), six keys, green LED,
//! CP2102 console. No display: the headless UI drives it with the keys,
//! tones and a Wi-Fi web page; the piano is reached over BLE.
//!
//! Keys (vendor spec, ESP-ADF board package "esp32_audio_kit_v2_3"):
//! KEY1 GPIO36 (input only, no pull-up), KEY2 GPIO13, KEY3 GPIO19,
//! KEY4 GPIO23, KEY5 GPIO18, KEY6 GPIO5 — all active low. Green LED GPIO22.
//! Codec and SD bring-up are in components/audiokit_bsp (C).

use std::sync::Mutex;

use esp_idf_svc::sys::bsp;
use log::{info, warn};

use super::key::Key;
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP32-Audio-Kit",
    backlight: false,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "KEY2 or RST",
    key_hint: "",
    touch: false,
};

/// ES8388 at the synth's own 24 kHz, stereo: the two speaker outputs take
/// the left and right channel (a mono stream would only reach one).
pub const AUDIO_OUT_RATE: u32 = 24_000;
pub const AUDIO_CHANNELS: u32 = 2;
/// Twice the synth's default: songs then peak around -6 dBFS on the small
/// speaker (loud chords saturate softly instead of clipping).
pub const SYNTH_GAIN: f32 = 1.6;

const KEY_GPIO: [i32; 6] = [36, 13, 19, 23, 18, 5];
const LED: i32 = 22;

static KEYS: Mutex<[Key; 6]> = Mutex::new([
    Key::new(KEY_GPIO[0]),
    Key::new(KEY_GPIO[1]),
    Key::new(KEY_GPIO[2]),
    Key::new(KEY_GPIO[3]),
    Key::new(KEY_GPIO[4]),
    Key::new(KEY_GPIO[5]),
]);

pub fn early_init() {
    unsafe {
        let mut c: bsp::gpio_config_t = core::mem::zeroed();
        c.pin_bit_mask = 1u64 << LED;
        c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
        bsp::gpio_config(&c);
        bsp::gpio_set_level(LED, 1);
    }
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn mount_sd() -> bool {
    let err = unsafe { bsp::audiokit_sdcard_mount() };
    if err == 0 {
        info!("microSD mounted at /sdcard");
        true
    } else {
        warn!("no microSD (mount error {err})");
        false
    }
}

pub fn speaker_codec() -> Option<bsp::esp_codec_dev_handle_t> {
    let h = unsafe { bsp::audiokit_speaker_init() };
    (!h.is_null()).then_some(h)
}

/// The ES8388 driver switches the amplifier (GPIO21) with the codec.
pub fn speaker_enable(_on: bool) {}

/// The green LED (status: on = piano connected, blinking = searching); no
/// colour.
pub fn status_led(on: bool, _rgb: [u8; 3]) {
    unsafe {
        bsp::gpio_set_level(LED, on as u32);
    }
}

pub fn set_backlight(_percent: i32) {}

/// No power switch: LED off and deep sleep; KEY2 (GPIO13, an RTC pin with
/// an RTC pull-up) or RST wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, KEY2 or RST wakes");
    status_led(false, [0; 3]);
    unsafe {
        while bsp::gpio_get_level(13) == 0 {
            bsp::vTaskDelay(10);
        }
        bsp::rtc_gpio_pullup_en(13);
        bsp::esp_sleep_enable_ext0_wakeup(13, 0);
        bsp::esp_deep_sleep_start()
    }
}

pub fn reboot() -> ! {
    unsafe { bsp::esp_restart() }
}

pub fn bt_controller_start(_status: &Status) -> anyhow::Result<()> {
    Ok(())
}

/// KEY1…KEY6 taps and holds as `Keypad(n)` / `KeypadHold(n)` (n = 1…6).
pub fn poll_buttons() -> Vec<Button> {
    let mut v = Vec::new();
    for (i, k) in KEYS.lock().unwrap().iter_mut().enumerate() {
        match k.poll() {
            Some(Button::KeyTap) | Some(Button::KeyDoubleTap) => v.push(Button::Keypad(i as u8 + 1)),
            Some(Button::KeyHold) => v.push(Button::KeypadHold(i as u8 + 1)),
            _ => {}
        }
    }
    v
}
