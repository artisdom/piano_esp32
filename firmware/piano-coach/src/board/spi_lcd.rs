//! Small SPI LCD panels (UI=mini boards): bus + panel IO setup, band writes
//! with completion wait, and a PWM backlight. The panel-specific part (which
//! esp_lcd driver, orientation, colour order) stays in the board module.
//!
//! Slint renders a band of lines into a DMA-capable buffer; [`SpiLcd::draw`]
//! byte-swaps it (the panels take RGB565 big-endian), queues it and waits on
//! the transfer-done interrupt, so the buffer is free again on return.

use core::ffi::c_void;

use esp_idf_svc::sys::{bsp, esp};
use log::info;

pub struct SpiLcd {
    pub panel: bsp::esp_lcd_panel_handle_t,
    /// Binary semaphore given by the transfer-done callback.
    done: bsp::QueueHandle_t,
}

// The handles are only used from the UI thread; the semaphore is ISR-safe.
unsafe impl Send for SpiLcd {}

pub struct Pins {
    pub host: bsp::spi_host_device_t,
    pub sclk: i32,
    pub mosi: i32,
    pub cs: i32,
    pub dc: i32,
    pub pclk_hz: u32,
    /// Largest single transfer (bytes): one band.
    pub max_transfer: usize,
}

const QUEUE_TYPE_BINARY_SEMAPHORE: u8 = 3;

unsafe extern "C" fn on_trans_done(
    _io: bsp::esp_lcd_panel_io_handle_t,
    _edata: *mut bsp::esp_lcd_panel_io_event_data_t,
    ctx: *mut c_void,
) -> bool {
    let mut woken: bsp::BaseType_t = 0;
    bsp::xQueueGiveFromISR(ctx as bsp::QueueHandle_t, &mut woken);
    woken != 0
}

/// SPI bus + panel IO (mode 0, 8-bit commands/params). The board then
/// creates its panel driver on `io` and calls [`SpiLcd::finish`].
pub fn open_io(p: &Pins) -> anyhow::Result<(bsp::esp_lcd_panel_io_handle_t, bsp::QueueHandle_t)> {
    unsafe {
        let mut bus: bsp::spi_bus_config_t = core::mem::zeroed();
        bus.__bindgen_anon_1.mosi_io_num = p.mosi;
        bus.__bindgen_anon_2.miso_io_num = -1;
        bus.sclk_io_num = p.sclk;
        bus.__bindgen_anon_3.quadwp_io_num = -1;
        bus.__bindgen_anon_4.quadhd_io_num = -1;
        bus.data4_io_num = -1;
        bus.data5_io_num = -1;
        bus.data6_io_num = -1;
        bus.data7_io_num = -1;
        bus.max_transfer_sz = p.max_transfer as i32;
        esp!(bsp::spi_bus_initialize(p.host, &bus, bsp::spi_common_dma_t_SPI_DMA_CH_AUTO))?;

        let done = bsp::xQueueGenericCreate(1, 0, QUEUE_TYPE_BINARY_SEMAPHORE);
        anyhow::ensure!(!done.is_null(), "no memory for the LCD semaphore");

        let mut cfg: bsp::esp_lcd_panel_io_spi_config_t = core::mem::zeroed();
        cfg.cs_gpio_num = p.cs;
        cfg.dc_gpio_num = p.dc;
        cfg.spi_mode = 0;
        cfg.pclk_hz = p.pclk_hz;
        cfg.trans_queue_depth = 4;
        cfg.lcd_cmd_bits = 8;
        cfg.lcd_param_bits = 8;
        cfg.on_color_trans_done = Some(on_trans_done);
        cfg.user_ctx = done as *mut c_void;
        let mut io = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_io_spi(p.host as bsp::esp_lcd_spi_bus_handle_t, &cfg, &mut io))?;
        Ok((io, done))
    }
}

impl SpiLcd {
    /// Wrap the board's initialised panel, clear it to `bg565` and turn the
    /// display on.
    pub fn finish(
        panel: bsp::esp_lcd_panel_handle_t,
        io: bsp::esp_lcd_panel_io_handle_t,
        done: bsp::QueueHandle_t,
        width: usize,
        height: usize,
        bg565: u16,
    ) -> anyhow::Result<SpiLcd> {
        let _ = io; // owned by the panel driver from here on
        let lcd = SpiLcd { panel, done };
        // Small enough to come from internal (DMA-capable) heap everywhere.
        let mut band = vec![0u16; width * 4];
        for y in (0..height).step_by(4) {
            let h = 4.min(height - y);
            band[..width * h].fill(bg565);
            lcd.draw(0, y, width, h, &mut band[..width * h]);
        }
        unsafe { esp!(bsp::esp_lcd_panel_disp_on_off(panel, true))? };
        info!("display up: {width}x{height} SPI panel");
        Ok(lcd)
    }

    /// Write a w×h block (native-endian RGB565, swapped in place) at (x, y)
    /// and wait until the transfer is done.
    pub fn draw(&self, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
        let n = w * h;
        debug_assert!(px.len() >= n);
        for p in &mut px[..n] {
            *p = p.swap_bytes();
        }
        unsafe {
            let r = bsp::esp_lcd_panel_draw_bitmap(
                self.panel,
                x as i32,
                y as i32,
                (x + w) as i32,
                (y + h) as i32,
                px.as_ptr() as *const c_void,
            );
            if r == 0 {
                // 100 ms: far longer than any band takes at ≥ 20 MHz.
                bsp::xQueueSemaphoreTake(self.done, 100);
            }
        }
    }
}

/// A band buffer in internal DMA-capable RAM, kept for the program's life.
pub fn alloc_band(pixels: usize) -> &'static mut [u16] {
    unsafe {
        let p = bsp::heap_caps_malloc(pixels * 2, bsp::MALLOC_CAP_DMA | bsp::MALLOC_CAP_INTERNAL) as *mut u16;
        assert!(!p.is_null(), "no DMA memory for a {pixels}-pixel LCD band");
        core::slice::from_raw_parts_mut(p, pixels)
    }
}

// ---- backlight (LEDC PWM) ---------------------------------------------------

const BL_TIMER: bsp::ledc_timer_t = bsp::ledc_timer_t_LEDC_TIMER_0;
const BL_CHANNEL: bsp::ledc_channel_t = bsp::ledc_channel_t_LEDC_CHANNEL_0;

pub fn backlight_init(gpio: i32) -> anyhow::Result<()> {
    unsafe {
        let mut t: bsp::ledc_timer_config_t = core::mem::zeroed();
        t.speed_mode = bsp::ledc_mode_t_LEDC_LOW_SPEED_MODE;
        t.duty_resolution = bsp::ledc_timer_bit_t_LEDC_TIMER_10_BIT;
        t.timer_num = BL_TIMER;
        t.freq_hz = 20_000;
        t.clk_cfg = bsp::soc_periph_ledc_clk_src_legacy_t_LEDC_AUTO_CLK;
        esp!(bsp::ledc_timer_config(&t))?;
        let mut c: bsp::ledc_channel_config_t = core::mem::zeroed();
        c.gpio_num = gpio;
        c.speed_mode = bsp::ledc_mode_t_LEDC_LOW_SPEED_MODE;
        c.channel = BL_CHANNEL;
        c.timer_sel = BL_TIMER;
        c.duty = 0;
        esp!(bsp::ledc_channel_config(&c))?;
    }
    Ok(())
}

/// 0–100 %. Never fully dark above 0 (the panels barely show below ~5 %).
pub fn backlight_set(percent: i32) {
    let p = percent.clamp(0, 100) as u32;
    let duty = if p == 0 { 0 } else { (p * 1023 / 100).max(40) };
    unsafe {
        bsp::ledc_set_duty(bsp::ledc_mode_t_LEDC_LOW_SPEED_MODE, BL_CHANNEL, duty);
        bsp::ledc_update_duty(bsp::ledc_mode_t_LEDC_LOW_SPEED_MODE, BL_CHANNEL);
    }
}
