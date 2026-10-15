//! CST816 (S/D) capacitive touch controller over I2C, as on the
//! ESP32-2424S012C and the LILYGO T-Display-S3 Touch. Polled: registers
//! 0x02.. hold the finger count and X/Y (12 bits each).

use esp_idf_svc::sys::{bsp, esp};
use log::info;

pub const ADDR: u16 = 0x15;

pub struct Cst816(bsp::i2c_master_dev_handle_t);

// Only used from the UI thread (behind a Mutex in the board module).
unsafe impl Send for Cst816 {}

pub struct Pins {
    pub sda: i32,
    pub scl: i32,
    /// Reset line, or -1.
    pub rst: i32,
}

impl Cst816 {
    /// Reset the controller, open the I2C bus and keep it awake (it stops
    /// answering in its auto low-power mode). Fails if nothing answers.
    pub fn open(p: &Pins) -> anyhow::Result<Cst816> {
        unsafe {
            if p.rst >= 0 {
                let mut c: bsp::gpio_config_t = core::mem::zeroed();
                c.pin_bit_mask = 1u64 << p.rst;
                c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
                bsp::gpio_config(&c);
                bsp::gpio_set_level(p.rst, 0);
                bsp::vTaskDelay(10);
                bsp::gpio_set_level(p.rst, 1);
                bsp::vTaskDelay(60);
            }
            let mut bc: bsp::i2c_master_bus_config_t = core::mem::zeroed();
            bc.i2c_port = 0;
            bc.sda_io_num = p.sda;
            bc.scl_io_num = p.scl;
            bc.__bindgen_anon_1.clk_source = bsp::soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT;
            bc.glitch_ignore_cnt = 7;
            bc.flags.set_enable_internal_pullup(1);
            let mut bus = core::ptr::null_mut();
            esp!(bsp::i2c_new_master_bus(&bc, &mut bus))?;
            let mut dc: bsp::i2c_device_config_t = core::mem::zeroed();
            dc.dev_addr_length = bsp::i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7;
            dc.device_address = ADDR;
            dc.scl_speed_hz = 400_000;
            let mut dev = core::ptr::null_mut();
            esp!(bsp::i2c_master_bus_add_device(bus, &dc, &mut dev))?;
            esp!(bsp::i2c_master_transmit(dev, [0xFEu8, 0xFF].as_ptr(), 2, 50))?;
            info!("touch: CST816 at 0x{ADDR:02x}");
            Ok(Cst816(dev))
        }
    }

    /// Raw finger position (the controller's own axes), if touched.
    pub fn read(&self) -> Option<(u16, u16)> {
        let mut b = [0u8; 5];
        let r = unsafe { bsp::i2c_master_transmit_receive(self.0, [0x02u8].as_ptr(), 1, b.as_mut_ptr(), b.len(), 20) };
        if r != 0 || b[0] & 0x0f == 0 {
            return None;
        }
        Some((((b[1] as u16 & 0x0f) << 8) | b[2] as u16, ((b[3] as u16 & 0x0f) << 8) | b[4] as u16))
    }
}
