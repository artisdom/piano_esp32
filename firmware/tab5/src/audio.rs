//! Tab5 speaker output: ES8388 codec (BSP I2S + esp_codec_dev) and the
//! NS4150B amplifier, fed by `piano_core::synth::Synth`.
//!
//! One thread owns the codec. The player thread drives the shared synth
//! (note on/off, pedal); this thread renders it in 256-sample blocks and
//! writes them to I2S, whose DMA paces the loop. The codec and amp are open
//! only while voices sound and close after 2 s of silence (an open, idle
//! amp hisses — herdline's beeps open per sound for the same reason).

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::sys::bsp;
use log::{info, warn};
use piano_core::synth::Synth;

/// 24 kHz: a piano voice has little above 10 kHz, and it saves a quarter
/// of the synth work compared to 32 kHz.
pub const SAMPLE_RATE: u32 = 24_000;
const BLOCK: usize = 256;

/// Speaker volume 0–100 (codec output volume).
pub static VOLUME: AtomicI32 = AtomicI32::new(70);

pub type SharedSynth = Arc<Mutex<Synth>>;

pub fn start() -> SharedSynth {
    let synth = Arc::new(Mutex::new(Synth::new(SAMPLE_RATE)));
    let s = synth.clone();
    // Core 1, above the UI/player priority: rendering a busy screen must
    // never starve the audio DMA (underruns are audible).
    let cfg = ThreadSpawnConfiguration {
        name: Some(c"audio"),
        stack_size: 12 * 1024,
        priority: 12,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    };
    if let Err(e) = cfg.set() {
        warn!("audio: thread config: {e}");
    }
    let r = std::thread::Builder::new().stack_size(12 * 1024).spawn(move || unsafe { run(s) });
    let _ = ThreadSpawnConfiguration::default().set();
    if let Err(e) = r {
        warn!("audio: thread spawn failed — {e}");
    }
    synth
}

unsafe fn run(synth: SharedSynth) {
    // The codec sits on the BSP's I2C bus, created by the display bring-up.
    while !crate::board::DISPLAY_UP.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50));
    }
    if bsp::bsp_audio_init(core::ptr::null()) != 0 {
        warn!("audio: bsp_audio_init failed — speaker off");
        return;
    }
    let spk = bsp::bsp_audio_codec_speaker_init();
    if spk.is_null() {
        warn!("audio: speaker codec init failed — speaker off");
        return;
    }
    info!("audio: speaker codec up ({SAMPLE_RATE} Hz mono)");
    let mut fs: bsp::esp_codec_dev_sample_info_t = core::mem::zeroed();
    fs.bits_per_sample = 16;
    fs.channel = 1;
    fs.sample_rate = SAMPLE_RATE;

    let mut buf = [0i16; BLOCK];
    let mut open = false;
    let mut silent_since = Instant::now();
    let mut vol_set = -1;
    let (mut busy_us, mut blocks) = (0u64, 0u32);
    loop {
        let active = synth.lock().unwrap().active_voices() > 0;
        if !open {
            if !active {
                std::thread::sleep(Duration::from_millis(3));
                continue;
            }
            let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_SPEAKER, true);
            if bsp::esp_codec_dev_open(spk, &mut fs) != 0 {
                warn!("audio: codec open failed");
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            open = true;
            vol_set = -1;
        }
        let vol = VOLUME.load(Ordering::Relaxed).clamp(0, 100);
        if vol != vol_set {
            let _ = bsp::esp_codec_dev_set_out_vol(spk, vol);
            vol_set = vol;
        }
        let t = Instant::now();
        synth.lock().unwrap().render(&mut buf);
        busy_us += t.elapsed().as_micros() as u64;
        blocks += 1;
        if blocks == 1000 {
            // Render cost vs. the 8 ms a block lasts — CPU headroom check.
            info!(
                "audio: synth {:.1}% of real time (one core)",
                busy_us as f32 / (blocks as f32 * BLOCK as f32 * 1e6 / SAMPLE_RATE as f32) * 100.0
            );
            (busy_us, blocks) = (0, 0);
        }
        // Blocks until the I2S DMA has room: this paces the loop.
        let _ = bsp::esp_codec_dev_write(spk, buf.as_mut_ptr() as *mut _, (BLOCK * 2) as i32);
        if active {
            silent_since = Instant::now();
        } else if silent_since.elapsed() > Duration::from_secs(2) {
            let _ = bsp::esp_codec_dev_close(spk);
            let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_SPEAKER, false);
            open = false;
        }
    }
}
