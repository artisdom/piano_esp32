//! XPT2046 resistive touch controller, as on the ESP32-2432S028 ("Cheap
//! Yellow Display"), bit-banged: its pins are not on an SPI host's IOMUX
//! and a touch is a few dozen bits per UI frame. PENIRQ (low while
//! pressed) gates the reads.
//!
//! Each measurement is a control byte (start, channel, 12-bit, differential
//! reference, power-down mode) followed by 16 clocks reading the result
//! (12 bits after one busy bit). The last command powers the ADC down with
//! PENIRQ enabled again.

use esp_idf_svc::sys::bsp;

pub struct Pins {
    pub clk: i32,
    pub mosi: i32,
    pub miso: i32,
    pub cs: i32,
    pub irq: i32,
}

pub struct Xpt2046(Pins);

/// Pressure (Z) below this is no touch (or a too-light one, noisy X/Y).
const Z_MIN: i32 = 350;

impl Xpt2046 {
    pub fn open(p: Pins) -> Xpt2046 {
        unsafe {
            let mut c: bsp::gpio_config_t = core::mem::zeroed();
            c.pin_bit_mask = 1u64 << p.clk | 1u64 << p.mosi | 1u64 << p.cs;
            c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
            bsp::gpio_config(&c);
            // MISO and PENIRQ are input-only pins (GPIO39/36) with an
            // external pull-up on PENIRQ.
            let mut c: bsp::gpio_config_t = core::mem::zeroed();
            c.pin_bit_mask = 1u64 << p.miso | 1u64 << p.irq;
            c.mode = bsp::gpio_mode_t_GPIO_MODE_INPUT;
            bsp::gpio_config(&c);
            bsp::gpio_set_level(p.cs, 1);
            bsp::gpio_set_level(p.clk, 0);
        }
        Xpt2046(p)
    }

    /// One conversion: send `cmd`, clock in the 12-bit result.
    fn xfer(&self, cmd: u8) -> i32 {
        let p = &self.0;
        unsafe {
            for i in (0..8).rev() {
                bsp::gpio_set_level(p.mosi, (cmd >> i & 1) as u32);
                bsp::gpio_set_level(p.clk, 1);
                bsp::gpio_set_level(p.clk, 0);
            }
            bsp::gpio_set_level(p.mosi, 0);
            let mut v = 0i32;
            for _ in 0..16 {
                bsp::gpio_set_level(p.clk, 1);
                v = v << 1 | bsp::gpio_get_level(p.miso);
                bsp::gpio_set_level(p.clk, 0);
            }
            v >> 3 & 0xfff
        }
    }

    /// Raw (X, Y) readings 0..4095 while pressed firmly enough: X is the
    /// 0xD1 channel, Y the 0x91 one (each the closer two of three samples,
    /// averaged).
    pub fn read(&self) -> Option<(i32, i32)> {
        let p = &self.0;
        if unsafe { bsp::gpio_get_level(p.irq) } != 0 {
            return None;
        }
        unsafe { bsp::gpio_set_level(p.cs, 0) };
        let z1 = self.xfer(0xB1);
        let z2 = self.xfer(0xC1);
        let z = z1 + 4095 - z2;
        let mut xy = None;
        if z >= Z_MIN {
            self.xfer(0x91); // the first sample after switching is noisy
            let xs = [self.xfer(0xD1), self.xfer(0xD1), self.xfer(0xD1)];
            let ys = [self.xfer(0x91), self.xfer(0x91), self.xfer(0x91)];
            xy = Some((best_two(xs), best_two(ys)));
        }
        self.xfer(0xD0); // power down, PENIRQ on
        unsafe { bsp::gpio_set_level(p.cs, 1) };
        xy
    }
}

/// The mean of the two closest of three samples (drops an outlier).
fn best_two(s: [i32; 3]) -> i32 {
    let (a, b, c) = (s[0], s[1], s[2]);
    let (dab, dac, dbc) = ((a - b).abs(), (a - c).abs(), (b - c).abs());
    if dab <= dac && dab <= dbc {
        (a + b) / 2
    } else if dac <= dbc {
        (a + c) / 2
    } else {
        (b + c) / 2
    }
}
