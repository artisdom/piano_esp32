//! One push button (the small boards' BOOT key, active low) turned into
//! tap / double tap / hold, polled from the UI loop (every frame, ≤ 40 ms).
//!
//! A tap is reported once the double-tap window has passed, so a double tap
//! never also produces a tap.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use esp_idf_svc::sys::bsp;

use super::Button;

const DOUBLE_WINDOW: Duration = Duration::from_millis(280);
const HOLD: Duration = Duration::from_millis(600);
const DEBOUNCE: Duration = Duration::from_millis(25);

pub struct Key {
    gpio: i32,
    ready: bool,
    down_since: Option<Instant>,
    held: bool,
    /// Release time of a tap that may still become a double tap.
    pending: Option<Instant>,
    last_change: Option<Instant>,
}

impl Key {
    pub const fn new(gpio: i32) -> Key {
        Key {
            gpio,
            ready: false,
            down_since: None,
            held: false,
            pending: None,
            last_change: None,
        }
    }

    fn setup(&mut self) {
        unsafe {
            let mut c: bsp::gpio_config_t = core::mem::zeroed();
            c.pin_bit_mask = 1u64 << self.gpio;
            c.mode = bsp::gpio_mode_t_GPIO_MODE_INPUT;
            c.pull_up_en = bsp::gpio_pullup_t_GPIO_PULLUP_ENABLE;
            bsp::gpio_config(&c);
        }
        self.ready = true;
    }

    pub fn poll(&mut self) -> Option<Button> {
        if !self.ready {
            self.setup();
        }
        let now = Instant::now();
        let down = unsafe { bsp::gpio_get_level(self.gpio) } == 0;
        let was_down = self.down_since.is_some();
        if down != was_down && self.last_change.is_none_or(|t| now.duration_since(t) >= DEBOUNCE) {
            self.last_change = Some(now);
            if down {
                self.down_since = Some(now);
                self.held = false;
            } else {
                self.down_since = None;
                if !self.held {
                    if self.pending.take().is_some() {
                        return Some(Button::KeyDoubleTap);
                    }
                    self.pending = Some(now);
                }
            }
        }
        if let Some(t) = self.down_since {
            if !self.held && now.duration_since(t) >= HOLD {
                self.held = true;
                self.pending = None;
                return Some(Button::KeyHold);
            }
        } else if self.pending.is_some_and(|t| now.duration_since(t) >= DOUBLE_WINDOW) {
            self.pending = None;
            return Some(Button::KeyTap);
        }
        None
    }
}

/// Poll a board's key kept in a static.
pub fn poll(key: &Mutex<Key>) -> Vec<Button> {
    key.lock().unwrap().poll().into_iter().collect()
}
