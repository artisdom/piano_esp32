//! BLE-MIDI central for the piano's `GZUT-MIDI` peripheral.
//!
//! Radio path: the NimBLE host runs here; the controller is either on-chip
//! (ESP32-S31) or on a coprocessor the board brings up first (Tab5: ESP32-C6
//! over ESP-Hosted, see board::bt_controller_start).
//!
//! Piano side (doc/GZUT_piano_protocol.md §2.2): service
//! 03B80E5A-EDE8-4B33-A751-6CE34EC4C700, characteristic
//! 7772E5DB-3868-4112-A1A9-F2669D106BF3 (read / write-without-response /
//! notify, CCCD at value handle + 1). Writes must start `80 80` and carry
//! plain MIDI — see `piano_core::blemidi`.
//!
//! NimBLE callbacks run on the NimBLE host task (8 KB stack) and only touch
//! atomics / channel sends; the manager thread does everything else.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use esp_idf_svc::sys::{bsp as nb, esp};
use log::{info, warn};
use piano_core::blemidi::{self, Packer};

use crate::link::{KeyEvent, Status};

const PIANO_NAME: &str = "GZUT-MIDI";
const MIDI_CHAR: &str = "7772e5db-3868-4112-a1a9-f2669d106bf3";
/// The BLE-MIDI service UUID 03b80e5a-ede8-4b33-a751-6ce34ec4c700 as it appears little-endian in advertising data.
const MIDI_SERVICE_LE: [u8; 16] = [
    0x00, 0xc7, 0xc4, 0x4e, 0xe3, 0x6c, 0x51, 0xa7, 0x33, 0x4b, 0xe8, 0xed, 0x5a, 0x0e, 0xb8, 0x03,
];
const NO_CONN: u16 = 0xffff;

static SYNCED: AtomicBool = AtomicBool::new(false);
static CONN: AtomicU16 = AtomicU16::new(NO_CONN);
static CONNECT_DONE: AtomicBool = AtomicBool::new(false);
static VAL_HANDLE: AtomicU16 = AtomicU16::new(0);
static MTU: AtomicU16 = AtomicU16::new(23);
static DISCONNECT_REASON: AtomicU16 = AtomicU16::new(0);
static CONNECT_STATUS: AtomicU16 = AtomicU16::new(0);
/// The notification (CCCD) write: 0 pending, 1 confirmed, 2 failed.
static CCCD_DONE: AtomicU16 = AtomicU16::new(0);

/// Least time between two MIDI writes. `ble_relaxed` boards gather 15 ms
/// of messages into one packet: a stream of small writes can take every
/// buffer of the original ESP32's small NimBLE pools, so the piano's
/// responses get lost and the host drops the link after 30 s (seen on the
/// ESP32-2432S028 as reason 0x216). Well under the 30–50 ms interval.
#[cfg(ble_relaxed)]
const WRITE_GAP: Duration = Duration::from_millis(15);

/// `ble_relaxed` link supervision timeout, in 10 ms units: 10 s.
#[cfg(ble_relaxed)]
const SUPERVISION_10MS: u16 = 1000;
#[cfg(not(ble_relaxed))]
const WRITE_GAP: Duration = Duration::ZERO;
static FOUND: Mutex<Option<nb::ble_addr_t>> = Mutex::new(None);
static RX: Mutex<Option<Sender<Vec<u8>>>> = Mutex::new(None);

pub fn start(out: Receiver<[u8; 3]>, keys: Sender<KeyEvent>) -> anyhow::Result<Arc<Status>> {
    let status = Arc::new(Status::default());
    *status.name.lock().unwrap() = "starting Bluetooth…".into();
    let st = status.clone();
    std::thread::Builder::new()
        .name("ble-midi".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            if let Err(e) = manager(out, keys, &st) {
                warn!("ble-midi: {e:#}");
                *st.name.lock().unwrap() = format!("Bluetooth error — {e}");
            }
        })?;
    Ok(status)
}

extern "C" fn host_task(_arg: *mut core::ffi::c_void) {
    unsafe {
        nb::nimble_port_run();
        nb::nimble_port_freertos_deinit();
    }
}

extern "C" fn on_sync() {
    unsafe {
        let _ = nb::ble_hs_util_ensure_addr(0);
    }
    SYNCED.store(true, Ordering::SeqCst);
}

/// Advertising data: our piano if it names itself GZUT-MIDI or lists the
/// BLE-MIDI service.
fn is_piano(adv: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < adv.len() {
        let len = adv[i] as usize;
        if len == 0 || i + 1 + len > adv.len() {
            break;
        }
        let (ty, p) = (adv[i + 1], &adv[i + 2..i + 1 + len]);
        match ty {
            0x08 | 0x09 if p == PIANO_NAME.as_bytes() => return true,
            0x06 | 0x07 if p.chunks_exact(16).any(|u| u == MIDI_SERVICE_LE) => return true,
            _ => {}
        }
        i += 1 + len;
    }
    false
}

extern "C" fn on_gap(event: *mut nb::ble_gap_event, _arg: *mut core::ffi::c_void) -> i32 {
    unsafe {
        let ev = &*event;
        match ev.type_ as u32 {
            nb::BLE_GAP_EVENT_DISC => {
                let d = &ev.__bindgen_anon_1.disc;
                let adv = core::slice::from_raw_parts(d.data, d.length_data as usize);
                if is_piano(adv) {
                    let mut f = FOUND.lock().unwrap();
                    if f.is_none() {
                        *f = Some(d.addr);
                    }
                }
            }
            nb::BLE_GAP_EVENT_CONNECT => {
                let c = &ev.__bindgen_anon_1.connect;
                if c.status == 0 {
                    CONN.store(c.conn_handle, Ordering::SeqCst);
                } else {
                    CONNECT_STATUS.store(c.status as u16, Ordering::SeqCst);
                }
                CONNECT_DONE.store(true, Ordering::SeqCst);
            }
            nb::BLE_GAP_EVENT_DISCONNECT => {
                // Reason: BLE_HS_ERR_HCI_BASE (0x200) + HCI code, e.g. 0x208
                // supervision timeout, 0x213 remote user terminated.
                DISCONNECT_REASON.store(ev.__bindgen_anon_1.disconnect.reason as u16, Ordering::SeqCst);
                CONN.store(NO_CONN, Ordering::SeqCst);
                VAL_HANDLE.store(0, Ordering::SeqCst);
            }
            nb::BLE_GAP_EVENT_CONN_UPDATE => {
                let conn = ev.__bindgen_anon_1.conn_update.conn_handle;
                log_conn_params(conn, "updated");
                #[cfg(ble_relaxed)]
                relax_link(conn);
            }
            // `ble_relaxed` boards keep the interval at 30–50 ms: shorter
            // ones starve Wi-Fi of airtime or drop on the original ESP32.
            #[cfg(ble_relaxed)]
            nb::BLE_GAP_EVENT_CONN_UPDATE_REQ | nb::BLE_GAP_EVENT_L2CAP_UPDATE_REQ => {
                let r = &ev.__bindgen_anon_1.conn_update_req;
                if !r.self_params.is_null() {
                    let s = &mut *r.self_params;
                    if s.itvl_max < 24 {
                        s.itvl_min = 24;
                        s.itvl_max = 40;
                    }
                    s.supervision_timeout = s.supervision_timeout.max(SUPERVISION_10MS);
                }
            }
            nb::BLE_GAP_EVENT_MTU => {
                MTU.store(ev.__bindgen_anon_1.mtu.value, Ordering::SeqCst);
            }
            nb::BLE_GAP_EVENT_NOTIFY_RX => {
                let n = &ev.__bindgen_anon_1.notify_rx;
                if n.attr_handle == VAL_HANDLE.load(Ordering::SeqCst) && !n.om.is_null() {
                    // Public host API (os_mbuf_copydata is not exported on
                    // every NimBLE port).
                    let mut buf = vec![0u8; 256];
                    let mut len = 0u16;
                    nb::ble_hs_mbuf_to_flat(n.om, buf.as_mut_ptr() as *mut _, 256, &mut len);
                    buf.truncate(len as usize);
                    if let Some(tx) = RX.lock().unwrap().as_ref() {
                        let _ = tx.send(buf);
                    }
                }
            }
            _ => {}
        }
    }
    0
}

extern "C" fn on_chr(
    _conn: u16,
    error: *const nb::ble_gatt_error,
    chr: *const nb::ble_gatt_chr,
    _arg: *mut core::ffi::c_void,
) -> i32 {
    unsafe {
        if !chr.is_null() && (*error).status == 0 {
            VAL_HANDLE.store((*chr).val_handle, Ordering::SeqCst);
        }
    }
    0
}

fn uuid(s: &str) -> anyhow::Result<nb::ble_uuid_any_t> {
    let mut u: nb::ble_uuid_any_t = unsafe { core::mem::zeroed() };
    let c = std::ffi::CString::new(s)?;
    let rc = unsafe { nb::ble_uuid_from_str(&mut u, c.as_ptr()) };
    anyhow::ensure!(rc == 0, "bad uuid {s}");
    Ok(u)
}

extern "C" fn on_cccd(
    _conn: u16,
    error: *const nb::ble_gatt_error,
    _attr: *mut nb::ble_gatt_attr,
    _arg: *mut core::ffi::c_void,
) -> i32 {
    let ok = unsafe { error.is_null() || (*error).status == 0 };
    CCCD_DONE.store(if ok { 1 } else { 2 }, Ordering::SeqCst);
    0
}

fn wait(what: &str, secs: u64, mut done: impl FnMut() -> bool) -> anyhow::Result<()> {
    let t = Instant::now();
    while !done() {
        anyhow::ensure!(t.elapsed() < Duration::from_secs(secs), "{what} timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// Bring up ESP-Hosted → C6 BT controller → NimBLE host.
fn bring_up(status: &Status) -> anyhow::Result<()> {
    // Coprocessor boards (Tab5: ESP32-C6 over ESP-Hosted) start their
    // controller here; native ones do it inside nimble_port_init.
    crate::board::bt_controller_start(status)?;
    unsafe {
        esp!(nb::nimble_port_init())?;
        // The original ESP32 sends at +3 dBm by default; its maximum, +9 dBm,
        // keeps the piano hearing boards with a weak antenna (the
        // ESP32-2432S028 saw the piano at −83 dBm a couple of metres away).
        #[cfg(esp32)]
        {
            use esp_idf_svc::sys as sys;
            for t in [
                sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_DEFAULT,
                sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_SCAN,
                sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_CONN_HDL0,
            ] {
                let _ = sys::esp_ble_tx_power_set(t, sys::esp_power_level_t_ESP_PWR_LVL_P9);
            }
        }
        // Quiet per-write INFO logs (also set in sdkconfig).
        esp_idf_svc::sys::esp_log_level_set(c"NimBLE".as_ptr(), esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN);
        (*core::ptr::addr_of_mut!(nb::ble_hs_cfg)).sync_cb = Some(on_sync);
        let _ = nb::ble_att_set_preferred_mtu(260);
        nb::nimble_port_freertos_init(Some(host_task));
    }
    wait("Bluetooth host sync", 15, || SYNCED.load(Ordering::SeqCst))?;
    info!("ble-midi: NimBLE host synced with the controller");
    Ok(())
}

fn scan() -> anyhow::Result<()> {
    *FOUND.lock().unwrap() = None;
    unsafe {
        let mut p: nb::ble_gap_disc_params = core::mem::zeroed();
        p.itvl = 0x50;
        p.window = 0x30;
        p.set_passive(0); // active: the name is in the scan response
        p.set_filter_duplicates(0);
        let rc = nb::ble_gap_disc(0, i32::MAX, &p, Some(on_gap), core::ptr::null_mut());
        anyhow::ensure!(rc == 0 || rc as u32 == nb::BLE_HS_EALREADY, "scan rc {rc}");
    }
    Ok(())
}

/// The piano renegotiates a 10 ms interval with a 500 ms supervision
/// timeout right after connecting (link-layer procedure, not offered to the
/// host). That drops the link on headless boards whenever Wi-Fi is busy,
/// and on the original ESP32 under the UI's load (ESP32-2432S028: gone
/// 5 s after connecting, then every service discovery timed out). Boards
/// built with `ble_relaxed` ask for 30–50 ms and a 10 s supervision timeout
/// instead (a hand between a weak-antenna board and the piano blanked the
/// link for over 4 s on the ESP32-2432S028), at most twice per connection
/// so the two sides cannot ping-pong.
#[cfg(ble_relaxed)]
fn relax_link(conn: u16) {
    static TRIES: AtomicU16 = AtomicU16::new(0);
    static FOR_CONN: AtomicU16 = AtomicU16::new(NO_CONN);
    if FOR_CONN.swap(conn, Ordering::SeqCst) != conn {
        TRIES.store(0, Ordering::SeqCst);
    }
    unsafe {
        let mut d: nb::ble_gap_conn_desc = core::mem::zeroed();
        if nb::ble_gap_conn_find(conn, &mut d) != 0 || (d.conn_itvl >= 24 && d.supervision_timeout >= SUPERVISION_10MS) {
            return;
        }
        if TRIES.fetch_add(1, Ordering::SeqCst) >= 2 {
            return;
        }
        let p = nb::ble_gap_upd_params {
            itvl_min: 24,
            itvl_max: 40,
            latency: 0,
            supervision_timeout: SUPERVISION_10MS,
            min_ce_len: 0x10,
            max_ce_len: 0x300,
        };
        let rc = nb::ble_gap_update_params(conn, &p);
        info!("ble-midi: asking for a 30–50 ms interval, 10 s supervision (rc {rc})");
    }
}

/// Connection interval, latency and supervision timeout in use.
fn log_conn_params(conn: u16, when: &str) {
    unsafe {
        let mut d: nb::ble_gap_conn_desc = core::mem::zeroed();
        if nb::ble_gap_conn_find(conn, &mut d) == 0 {
            info!(
                "ble-midi: link {when}: interval {:.2} ms, latency {}, supervision {} ms",
                d.conn_itvl as f32 * 1.25,
                d.conn_latency,
                d.supervision_timeout as u32 * 10
            );
        }
    }
}

fn connect(addr: nb::ble_addr_t) -> anyhow::Result<u16> {
    CONNECT_DONE.store(false, Ordering::SeqCst);
    CONN.store(NO_CONN, Ordering::SeqCst);
    unsafe {
        let _ = nb::ble_gap_disc_cancel();
        // `ble_relaxed` boards (Wi-Fi sharing the radio, original ESP32): a
        // 30–50 ms interval and a 10 s supervision timeout (default 2.56 s)
        // ride out busy moments instead of dropping the piano.
        #[cfg(ble_relaxed)]
        let params = nb::ble_gap_conn_params {
            scan_itvl: 0x10,
            scan_window: 0x10,
            itvl_min: 24,
            itvl_max: 40,
            latency: 0,
            supervision_timeout: SUPERVISION_10MS,
            // NimBLE's own defaults (0 is not accepted by every controller).
            min_ce_len: 0x10,
            max_ce_len: 0x300,
        };
        #[cfg(ble_relaxed)]
        let params_ptr: *const nb::ble_gap_conn_params = &params;
        #[cfg(not(ble_relaxed))]
        let params_ptr: *const nb::ble_gap_conn_params = core::ptr::null();
        let rc = nb::ble_gap_connect(0, &addr, 10_000, params_ptr, Some(on_gap), core::ptr::null_mut());
        anyhow::ensure!(rc == 0, "connect rc {rc}");
    }
    wait("connect", 12, || CONNECT_DONE.load(Ordering::SeqCst))?;
    let conn = CONN.load(Ordering::SeqCst);
    anyhow::ensure!(conn != NO_CONN, "connect failed (status 0x{:x})", CONNECT_STATUS.load(Ordering::SeqCst));
    // MTU exchange (the piano offers ~500; NimBLE caps at our preference).
    unsafe {
        let _ = nb::ble_gattc_exchange_mtu(conn, None, core::ptr::null_mut());
    }
    // The piano has a single characteristic with this UUID (inside
    // MIDI_SERVICE), so a whole-range search is unambiguous.
    let chr = uuid(MIDI_CHAR)?;
    VAL_HANDLE.store(0, Ordering::SeqCst);
    unsafe {
        esp!(nb::ble_gattc_disc_chrs_by_uuid(conn, 1, 0xffff, &chr.u, Some(on_chr), core::ptr::null_mut()))?;
    }
    wait("MIDI characteristic discovery", 8, || VAL_HANDLE.load(Ordering::SeqCst) != 0)?;
    let h = VAL_HANDLE.load(Ordering::SeqCst);
    // Notifications on: CCCD sits right after the value handle.
    // Wait for the piano's answer before sending anything else: MIDI
    // traffic must not crowd out the response.
    let cccd = [1u8, 0];
    CCCD_DONE.store(0, Ordering::SeqCst);
    unsafe {
        esp!(nb::ble_gattc_write_flat(conn, h + 1, cccd.as_ptr() as *const _, 2, Some(on_cccd), core::ptr::null_mut()))?;
    }
    wait("notification setup", 5, || CCCD_DONE.load(Ordering::SeqCst) != 0)?;
    if CCCD_DONE.load(Ordering::SeqCst) == 2 {
        warn!("ble-midi: the piano refused notifications (its keys will not reach us)");
    }
    info!("ble-midi: connected, MIDI value handle {h}, MTU {}", MTU.load(Ordering::SeqCst));
    log_conn_params(conn, "up");
    Ok(conn)
}

fn manager(out: Receiver<[u8; 3]>, keys: Sender<KeyEvent>, status: &Status) -> anyhow::Result<()> {
    let (rx_tx, rx_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    *RX.lock().unwrap() = Some(rx_tx);
    bring_up(status)?;

    loop {
        *status.name.lock().unwrap() = format!("scanning for {PIANO_NAME}…");
        status.connected.store(false, Ordering::SeqCst);
        scan()?;
        let addr = loop {
            if let Some(a) = *FOUND.lock().unwrap() {
                break a;
            }
            while out.try_recv().is_ok() {} // nothing to send to yet
            std::thread::sleep(Duration::from_millis(100));
        };
        *status.name.lock().unwrap() = format!("connecting to {PIANO_NAME}…");
        let conn = match connect(addr) {
            Ok(c) => c,
            Err(e) => {
                // Did the link itself go, or only the procedure?
                if CONN.load(Ordering::SeqCst) == NO_CONN && CONNECT_DONE.load(Ordering::SeqCst) {
                    warn!("ble-midi: {e:#} (link lost, reason 0x{:03x})", DISCONNECT_REASON.load(Ordering::SeqCst));
                } else {
                    warn!("ble-midi: {e:#}");
                }
                unsafe {
                    let _ = nb::ble_gap_terminate(CONN.load(Ordering::SeqCst), 0x13);
                }
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        *status.name.lock().unwrap() = PIANO_NAME.into();
        status.connected.store(true, Ordering::SeqCst);
        while rx_rx.try_recv().is_ok() {}

        // Connected: pump both directions until the link drops.
        let handle = VAL_HANDLE.load(Ordering::SeqCst);
        let limit = (MTU.load(Ordering::SeqCst) as usize).saturating_sub(3);
        let mut packer = Packer::new(limit);
        let mut pending: Option<Vec<u8>> = None;
        let mut last_write = Instant::now();
        let mut last_rssi = Instant::now();
        while CONN.load(Ordering::SeqCst) == conn {
            while let Ok(pkt) = rx_rx.try_recv() {
                blemidi::decode(&pkt, |m| match m {
                    [s, k, v] if s & 0xf0 == 0x90 && *v > 0 => {
                        let _ = keys.send(KeyEvent::Down { key: *k, vel: *v });
                    }
                    [s, k, _] if s & 0xf0 == 0x80 || s & 0xf0 == 0x90 => {
                        let _ = keys.send(KeyEvent::Up { key: *k });
                    }
                    [0xf0, 0x52, 0x4b, p, v, 0, 0, 0xf7] => {
                        *status.panel.lock().unwrap() = Some((*p, *v));
                    }
                    _ => {}
                });
            }
            // Gather everything queued into as few packets as possible.
            if pending.is_none() && last_write.elapsed() >= WRITE_GAP {
                while let Ok(m) = out.try_recv() {
                    if let Some(full) = packer.push(&m) {
                        pending = Some(full);
                        break;
                    }
                }
                if pending.is_none() {
                    pending = packer.flush();
                }
            }
            if let Some(pkt) = pending.as_ref() {
                let rc = unsafe {
                    nb::ble_gattc_write_no_rsp_flat(conn, handle, pkt.as_ptr() as *const _, pkt.len() as u16)
                };
                if rc == 0 {
                    pending = None;
                    last_write = Instant::now();
                } else if rc as u32 != nb::BLE_HS_ENOMEM && rc as u32 != nb::BLE_HS_EBUSY {
                    warn!("ble-midi: write rc {rc}");
                    pending = None;
                }
                // ENOMEM/EBUSY: controller buffers full — retry next round.
            }
            // Signal strength now and then: link drops are easier to judge.
            if last_rssi.elapsed() >= Duration::from_secs(30) {
                last_rssi = Instant::now();
                let mut rssi = 0i8;
                if unsafe { nb::ble_gap_conn_rssi(conn, &mut rssi) } == 0 {
                    info!("ble-midi: RSSI {rssi} dBm");
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        warn!("ble-midi: piano disconnected (reason 0x{:03x})", DISCONNECT_REASON.load(Ordering::SeqCst));
    }
}
