//! USB-MIDI 1.0 host class driver on the board's USB-A host port.
//!
//! ESP-IDF's USB host library has no MIDI class driver, so this is a small
//! client of its own: on attach it parses the configuration descriptor, finds
//! the Audio / MIDI-Streaming interface (class 1, subclass 3) and its bulk
//! endpoints, claims it, keeps one IN transfer permanently queued, and sends
//! outgoing messages through one OUT transfer.
//!
//! Everything — event callbacks and transfer callbacks — runs on the single
//! client task created by [`start`], so the state needs no locking.
//!
//! OUT pacing: the GZUT piano loses events when a large burst arrives at
//! once (88 Note Ons in one write lit only the upper keys; groups of 8 with
//! 20 ms gaps all lit — doc/GZUT_piano_protocol.md §2.1). Each transfer
//! carries at most [`MAX_EVENTS_PER_XFER`] events and transfers are spaced by
//! [`XFER_GAP`] while a backlog exists.

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use esp_idf_svc::sys::bsp;
use log::{info, warn};
use piano_core::usbmidi::{self, Incoming};

use crate::link::{KeyEvent, Status};

pub const MAX_EVENTS_PER_XFER: usize = 8;
pub const XFER_GAP: Duration = Duration::from_millis(12);
const MPS: usize = 64;

struct Dev {
    handle: bsp::usb_device_handle_t,
    iface: u8,
    ep_in: u8,
    ep_out: u8,
    xin: *mut bsp::usb_transfer_t,
    xout: *mut bsp::usb_transfer_t,
    in_busy: bool,
    out_busy: bool,
    gone: bool,
}

struct Ctx {
    client: bsp::usb_host_client_handle_t,
    new_dev: Option<u8>,
    gone_handle: Option<bsp::usb_device_handle_t>,
    dev: Option<Dev>,
    decoder: usbmidi::Decoder,
    keys: Sender<KeyEvent>,
    status: Arc<Status>,
}

unsafe extern "C" fn client_cb(msg: *const bsp::usb_host_client_event_msg_t, arg: *mut c_void) {
    let ctx = &mut *(arg as *mut Ctx);
    let msg = &*msg;
    if msg.event == bsp::usb_host_client_event_t_USB_HOST_CLIENT_EVENT_NEW_DEV {
        ctx.new_dev = Some(msg.__bindgen_anon_1.new_dev.address);
    } else if msg.event == bsp::usb_host_client_event_t_USB_HOST_CLIENT_EVENT_DEV_GONE {
        ctx.gone_handle = Some(msg.__bindgen_anon_1.dev_gone.dev_hdl);
    }
}

unsafe extern "C" fn in_cb(xfer: *mut bsp::usb_transfer_t) {
    let x = &mut *xfer;
    let ctx = &mut *(x.context as *mut Ctx);
    let Some(dev) = ctx.dev.as_mut() else { return };
    dev.in_busy = false;
    if x.status == bsp::usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED {
        let data = core::slice::from_raw_parts(x.data_buffer, x.actual_num_bytes as usize);
        let keys = &ctx.keys;
        let status = &ctx.status;
        ctx.decoder.feed(data, |m| match m {
            Incoming::Short(b) if b.len() >= 3 && b[0] & 0xf0 == 0x90 && b[2] > 0 => {
                let _ = keys.send(KeyEvent::Down { key: b[1], vel: b[2] });
            }
            Incoming::Short(b) if b.len() >= 3 && (b[0] & 0xf0 == 0x80 || b[0] & 0xf0 == 0x90) => {
                let _ = keys.send(KeyEvent::Up { key: b[1] });
            }
            Incoming::SysEx(sx) if sx.len() == 8 && sx[1] == 0x52 && sx[2] == 0x4b => {
                *status.panel.lock().unwrap() = Some((sx[3], sx[4]));
            }
            _ => {}
        });
    }
    if !dev.gone && x.status != bsp::usb_transfer_status_t_USB_TRANSFER_STATUS_NO_DEVICE {
        submit_in(dev);
    }
}

unsafe extern "C" fn out_cb(xfer: *mut bsp::usb_transfer_t) {
    let x = &mut *xfer;
    let ctx = &mut *(x.context as *mut Ctx);
    if let Some(dev) = ctx.dev.as_mut() {
        dev.out_busy = false;
        if x.status != bsp::usb_transfer_status_t_USB_TRANSFER_STATUS_COMPLETED {
            warn!("usb-midi: OUT transfer status {}", x.status);
        }
    }
}

unsafe fn submit_in(dev: &mut Dev) {
    let x = &mut *dev.xin;
    x.num_bytes = MPS as i32;
    if bsp::usb_host_transfer_submit(dev.xin) == 0 {
        dev.in_busy = true;
    } else {
        warn!("usb-midi: IN submit failed");
    }
}

/// Endpoints of the first MIDI-Streaming interface in a configuration.
fn find_midi(cfg: &[u8]) -> Option<(u8, u8, u8, u8)> {
    let (mut i, mut iface, mut alt, mut in_midi) = (0usize, 0u8, 0u8, false);
    let (mut ep_in, mut ep_out) = (None, None);
    while i + 2 <= cfg.len() {
        let len = cfg[i] as usize;
        if len < 2 || i + len > cfg.len() {
            break;
        }
        match cfg[i + 1] {
            4 if len >= 9 => {
                if in_midi && ep_in.is_some() && ep_out.is_some() {
                    break;
                }
                iface = cfg[i + 2];
                alt = cfg[i + 3];
                in_midi = cfg[i + 5] == 1 && cfg[i + 6] == 3;
                ep_in = None;
                ep_out = None;
            }
            5 if len >= 7 && in_midi => {
                let addr = cfg[i + 2];
                let kind = cfg[i + 3] & 3; // 2 bulk, 3 interrupt
                if kind == 2 || kind == 3 {
                    if addr & 0x80 != 0 {
                        ep_in.get_or_insert(addr);
                    } else {
                        ep_out.get_or_insert(addr);
                    }
                }
            }
            _ => {}
        }
        i += len;
    }
    match (in_midi, ep_in, ep_out) {
        (true, Some(a), Some(b)) => Some((iface, alt, a, b)),
        _ => None,
    }
}

unsafe fn product_name(handle: bsp::usb_device_handle_t) -> String {
    let mut info: bsp::usb_device_info_t = core::mem::zeroed();
    if bsp::usb_host_device_info(handle, &mut info) != 0 || info.str_desc_product.is_null() {
        return "USB MIDI".into();
    }
    let d = info.str_desc_product as *const u8;
    let len = *d as usize;
    let units: Vec<u16> = (2..len)
        .step_by(2)
        .map(|o| u16::from_le_bytes([*d.add(o), *d.add(o + 1)]))
        .collect();
    String::from_utf16_lossy(&units)
}

unsafe fn attach(ctx: &mut Ctx, addr: u8) {
    let mut handle: bsp::usb_device_handle_t = core::ptr::null_mut();
    if bsp::usb_host_device_open(ctx.client, addr, &mut handle) != 0 {
        warn!("usb-midi: cannot open device {addr}");
        return;
    }
    let mut ddesc: *const bsp::usb_device_desc_t = core::ptr::null();
    let mut cdesc: *const bsp::usb_config_desc_t = core::ptr::null();
    if bsp::usb_host_get_device_descriptor(handle, &mut ddesc) != 0
        || bsp::usb_host_get_active_config_descriptor(handle, &mut cdesc) != 0
        || cdesc.is_null()
    {
        let _ = bsp::usb_host_device_close(ctx.client, handle);
        return;
    }
    let raw = cdesc as *const u8;
    let total = u16::from_le_bytes([*raw.add(2), *raw.add(3)]) as usize;
    let cfg = core::slice::from_raw_parts(raw, total);
    let (vid, pid) = {
        let d = ddesc as *const u8;
        (
            u16::from_le_bytes([*d.add(8), *d.add(9)]),
            u16::from_le_bytes([*d.add(10), *d.add(11)]),
        )
    };
    let Some((iface, alt, ep_in, ep_out)) = find_midi(cfg) else {
        info!("usb: device {vid:04x}:{pid:04x} is not USB-MIDI — ignored");
        let _ = bsp::usb_host_device_close(ctx.client, handle);
        return;
    };
    if bsp::usb_host_interface_claim(ctx.client, handle, iface, alt) != 0 {
        warn!("usb-midi: interface {iface} claim failed");
        let _ = bsp::usb_host_device_close(ctx.client, handle);
        return;
    }
    let mut xin = core::ptr::null_mut();
    let mut xout = core::ptr::null_mut();
    if bsp::usb_host_transfer_alloc(MPS, 0, &mut xin) != 0
        || bsp::usb_host_transfer_alloc(MPS, 0, &mut xout) != 0
    {
        warn!("usb-midi: transfer alloc failed");
        let _ = bsp::usb_host_interface_release(ctx.client, handle, iface);
        let _ = bsp::usb_host_device_close(ctx.client, handle);
        return;
    }
    let ctx_ptr = ctx as *mut Ctx as *mut c_void;
    for (x, ep, cb) in [
        (xin, ep_in, in_cb as unsafe extern "C" fn(*mut bsp::usb_transfer_t)),
        (xout, ep_out, out_cb),
    ] {
        (*x).device_handle = handle;
        (*x).bEndpointAddress = ep;
        (*x).callback = Some(cb);
        (*x).context = ctx_ptr;
        (*x).timeout_ms = 0;
    }
    let name = product_name(handle);
    info!("usb-midi: {name} ({vid:04x}:{pid:04x}) iface {iface} IN {ep_in:#04x} OUT {ep_out:#04x}");
    *ctx.status.name.lock().unwrap() = name;
    ctx.status.connected.store(true, Ordering::SeqCst);
    ctx.decoder = usbmidi::Decoder::default();
    ctx.dev = Some(Dev {
        handle,
        iface,
        ep_in,
        ep_out,
        xin,
        xout,
        in_busy: false,
        out_busy: false,
        gone: false,
    });
    submit_in(ctx.dev.as_mut().unwrap());
}

/// Tear down once the device is gone and no transfer is in flight.
unsafe fn try_detach(ctx: &mut Ctx) {
    let ready = matches!(&ctx.dev, Some(d) if d.gone && !d.in_busy && !d.out_busy);
    if !ready {
        return;
    }
    let d = ctx.dev.take().unwrap();
    let _ = bsp::usb_host_transfer_free(d.xin);
    let _ = bsp::usb_host_transfer_free(d.xout);
    let _ = bsp::usb_host_interface_release(ctx.client, d.handle, d.iface);
    let _ = bsp::usb_host_device_close(ctx.client, d.handle);
    ctx.status.connected.store(false, Ordering::SeqCst);
    info!("usb-midi: device detached (ep {:#04x}/{:#04x})", d.ep_in, d.ep_out);
}

/// Power the USB-A port, install the host stack and run the MIDI client on
/// its own task. Messages received on `out` are sent to the piano; keyboard
/// events are delivered on `keys`.
pub fn start(out: Receiver<[u8; 3]>, keys: Sender<KeyEvent>) -> anyhow::Result<Arc<Status>> {
    crate::board::usb_host_start()?;
    let status = Arc::new(Status::default());
    let st = status.clone();
    std::thread::Builder::new()
        .name("usb-midi".into())
        .stack_size(8 * 1024)
        .spawn(move || unsafe { client_task(out, keys, st) })?;
    Ok(status)
}

unsafe fn client_task(out: Receiver<[u8; 3]>, keys: Sender<KeyEvent>, status: Arc<Status>) {
    let ctx: &mut Ctx = Box::leak(Box::new(Ctx {
        client: core::ptr::null_mut(),
        new_dev: None,
        gone_handle: None,
        dev: None,
        decoder: usbmidi::Decoder::default(),
        keys,
        status,
    }));
    let mut cfg: bsp::usb_host_client_config_t = core::mem::zeroed();
    cfg.is_synchronous = false;
    cfg.max_num_event_msg = 5;
    cfg.__bindgen_anon_1.async_.client_event_callback = Some(client_cb);
    cfg.__bindgen_anon_1.async_.callback_arg = ctx as *mut Ctx as *mut c_void;
    if bsp::usb_host_client_register(&cfg, &mut ctx.client) != 0 {
        warn!("usb-midi: client register failed");
        return;
    }
    info!("usb-midi: host client ready — waiting for the piano on USB-A");

    let mut backlog: std::collections::VecDeque<[u8; 3]> = Default::default();
    let mut last_out = Instant::now();
    loop {
        // 1 tick = 1 ms (CONFIG_FREERTOS_HZ=1000).
        bsp::usb_host_client_handle_events(ctx.client, 1);
        if let Some(addr) = ctx.new_dev.take() {
            if ctx.dev.is_none() {
                attach(ctx, addr);
            }
        }
        if let Some(h) = ctx.gone_handle.take() {
            if let Some(d) = ctx.dev.as_mut() {
                if d.handle == h {
                    d.gone = true;
                }
            }
        }
        try_detach(ctx);

        while let Ok(m) = out.try_recv() {
            backlog.push_back(m);
        }
        let Some(dev) = ctx.dev.as_mut() else {
            backlog.clear(); // nothing to send to
            continue;
        };
        if dev.gone || dev.out_busy || backlog.is_empty() || last_out.elapsed() < XFER_GAP {
            continue;
        }
        let x = &mut *dev.xout;
        let buf = core::slice::from_raw_parts_mut(x.data_buffer, MPS);
        let mut n = 0;
        while n < MAX_EVENTS_PER_XFER {
            let Some(m) = backlog.pop_front() else { break };
            if let Some(p) = usbmidi::encode(&m) {
                buf[n * 4..n * 4 + 4].copy_from_slice(&p);
                n += 1;
            }
        }
        if n == 0 {
            continue;
        }
        x.num_bytes = (n * 4) as i32;
        if bsp::usb_host_transfer_submit(dev.xout) == 0 {
            dev.out_busy = true;
            last_out = Instant::now();
        } else {
            warn!("usb-midi: OUT submit failed");
        }
    }
}
