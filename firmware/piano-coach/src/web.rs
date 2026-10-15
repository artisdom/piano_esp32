//! The headless boards' web page: a Wi-Fi access point ("PianoCoach-XXXX",
//! WPA2 password `pianocoach`) and an HTTP server at http://192.168.4.1
//! serving one page (`web/index.html`) and a small JSON API. A tiny DNS
//! server answers every name with the board and unknown pages redirect to
//! it, so phones treat the network as a captive portal and open the page
//! by themselves after joining.
//!
//!
//! * `GET /api/state` — piano link, song, position, mode, hands, tempo,
//!   speaker, score, playlist position
//! * `GET /api/dir?p=<folder>` — sub-folders and songs of a library folder
//! * `GET /api/search?q=<text>` — songs by pinyin initials, pinyin or text
//! * `GET /api/play?p=<song>` — make it the current song and start it
//! * `GET /api/cmd?c=<command>[&v=<value>]` — playpause, stop, restart,
//!   next, prev, mode (learn/follow/listen), hands (both/right/left), tempo,
//!   vol, sound (0 piano, 1 speaker, 2 both), ledtest, alloff
//!
//! Paths are relative to the library root; anything with `..` is refused.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::ipv4::{self, Mask, RouterConfiguration, Subnet};
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration, EspWifi, WifiDriver};
use std::net::{Ipv4Addr, UdpSocket};
use log::info;
use piano_core::engine::{Mode, State};
use piano_core::library::{self, fmt_time};
use piano_core::song::{key_name, HandSel};

use crate::headless::{list_dir, SharedCoach};
use crate::player;

/// web/index.html, gzipped by build.rs (≈ 3.5 KB instead of 11.6 KB over
/// a radio shared with Bluetooth).
const PAGE_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/index.html.gz"));
pub const PASSWORD: &str = "pianocoach";
const AP_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);

/// Keeps the Wi-Fi driver and the server alive.
pub struct Web {
    _wifi: EspWifi<'static>,
    _server: EspHttpServer<'static>,
}

// ---- small JSON writer ---------------------------------------------------------

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

// ---- query strings -------------------------------------------------------------

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn param(uri: &str, key: &str) -> Option<String> {
    let q = uri.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| decode(v))
    })
}

/// A library-relative path that stays inside the library.
fn safe_rel(rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel.trim_start_matches('/'));
    p.components().all(|c| matches!(c, Component::Normal(_))).then(|| p.to_path_buf())
}

// ---- API -----------------------------------------------------------------------

fn state_json(coach: &SharedCoach) -> String {
    let c = coach.lock().unwrap();
    let snap = c.shared.lock().unwrap().clone();
    let connected = c.status.connected.load(Ordering::Relaxed);
    let piano = c.status.name.lock().unwrap().clone();
    let song = c.current().map(|p| {
        let e = library::entry(&p.file_name().unwrap_or_default().to_string_lossy(), false);
        let folder = p
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| library::entry(&n.to_string_lossy(), true).title)
            .unwrap_or_default();
        format!(
            "{{\"title\":{},\"artist\":{},\"folder\":{},\"path\":{}}}",
            esc(&e.title),
            esc(&e.subtitle),
            esc(&folder),
            esc(&c.rel(&p))
        )
    });
    let dur = snap.song.as_ref().map(|s| s.duration_ms).unwrap_or(0);
    let state = match snap.state {
        State::Stopped => "stopped",
        State::Playing => "playing",
        State::Waiting => "waiting",
        State::Paused => "paused",
        State::Finished => "finished",
    };
    let mode = match c.opts.mode {
        Mode::Learn => "learn",
        Mode::Follow => "follow",
        Mode::Listen => "listen",
    };
    let hands = match c.opts.hands {
        HandSel::Both => "both",
        HandSel::Right => "right",
        HandSel::Left => "left",
    };
    let st = snap.stats;
    let prompt: Vec<String> = snap.waiting.iter().map(|&k| key_name(k)).collect();
    let indexed = c.search.lock().unwrap().as_ref().map(|i| i.songs.len() as i64).unwrap_or(-1);
    format!(
        "{{\"piano\":{{\"connected\":{connected},\"name\":{}}},\"sd\":{},\"song\":{},\"error\":{},\
         \"state\":\"{state}\",\"pos\":{},\"dur\":{},\"posText\":{},\"durText\":{},\
         \"mode\":\"{mode}\",\"hands\":\"{hands}\",\"tempo\":{},\"volume\":{},\"sound\":{},\
         \"stats\":{{\"acc\":{},\"done\":{},\"total\":{},\"hits\":{},\"streak\":{}}},\"prompt\":{},\
         \"playlist\":{{\"dir\":{},\"index\":{},\"count\":{}}},\"indexed\":{indexed}}}",
        esc(&piano),
        c.sd_ok,
        song.unwrap_or_else(|| "null".into()),
        snap.load_error.as_deref().map(esc).unwrap_or_else(|| "null".into()),
        snap.pos_ms,
        dur,
        esc(&fmt_time(snap.pos_ms)),
        esc(&fmt_time(dur)),
        c.opts.tempo_pct,
        crate::audio::VOLUME.load(Ordering::Relaxed),
        player::SOUND.load(Ordering::Relaxed),
        st.accuracy(),
        st.steps_done,
        st.steps_total,
        st.hits,
        st.streak,
        esc(&prompt.join(" ")),
        esc(&c.rel(&c.dir)),
        c.index,
        c.playlist.len()
    )
}

fn dir_json(coach: &SharedCoach, rel: &str) -> Option<String> {
    let root = coach.lock().unwrap().root.clone();
    let rel = safe_rel(rel)?;
    let dir = root.join(&rel);
    let (dirs, songs) = list_dir(&dir);
    let item = |n: &String, d: bool| {
        let e = library::entry(n, d);
        format!("{{\"name\":{},\"title\":{},\"sub\":{}}}", esc(n), esc(&e.title), esc(&e.subtitle))
    };
    let title = if rel.as_os_str().is_empty() {
        "Library".to_string()
    } else {
        library::entry(&rel.file_name().unwrap_or_default().to_string_lossy(), true).title
    };
    Some(format!(
        "{{\"path\":{},\"title\":{},\"dirs\":[{}],\"songs\":[{}]}}",
        esc(&rel.to_string_lossy()),
        esc(&title),
        dirs.iter().map(|n| item(n, true)).collect::<Vec<_>>().join(","),
        songs.iter().map(|n| item(n, false)).collect::<Vec<_>>().join(",")
    ))
}

fn search_json(coach: &SharedCoach, q: &str) -> String {
    let (root, slot) = {
        let c = coach.lock().unwrap();
        (c.root.clone(), c.search.clone())
    };
    let guard = slot.lock().unwrap();
    let Some(index) = guard.as_ref() else {
        return "{\"ready\":false,\"results\":[]}".into();
    };
    let hits = index.search(q, 100);
    let items: Vec<String> = hits
        .iter()
        .map(|s| {
            let rel = s.path.strip_prefix(&root).unwrap_or(&s.path).to_string_lossy().into_owned();
            let sub = if s.artist.is_empty() { s.folder.clone() } else { format!("{} · {}", s.artist, s.folder) };
            format!("{{\"path\":{},\"title\":{},\"sub\":{}}}", esc(&rel), esc(&s.title), esc(&sub))
        })
        .collect();
    format!("{{\"ready\":true,\"results\":[{}]}}", items.join(","))
}

fn command(coach: &SharedCoach, c: &str, v: Option<String>) -> bool {
    let mut co = coach.lock().unwrap();
    let num = || v.as_deref().and_then(|s| s.parse::<i32>().ok());
    match c {
        "playpause" => co.play_pause(),
        "stop" => co.stop(),
        "restart" => co.restart(),
        "next" => co.step(1),
        "prev" => co.step(-1),
        "mode" => co.set_mode(match v.as_deref() {
            Some("follow") => Mode::Follow,
            Some("listen") => Mode::Listen,
            _ => Mode::Learn,
        }),
        "hands" => co.set_hands(match v.as_deref() {
            Some("right") => HandSel::Right,
            Some("left") => HandSel::Left,
            _ => HandSel::Both,
        }),
        "tempo" => match num() {
            Some(t) => co.set_tempo(t),
            None => return false,
        },
        "vol" => match num() {
            Some(t) => co.set_volume(t),
            None => return false,
        },
        "sound" => match num() {
            Some(s) => co.set_sound(s.clamp(0, 2) as u8),
            None => return false,
        },
        "ledtest" => co.led_test(),
        "alloff" => co.all_off(),
        _ => return false,
    }
    true
}

// ---- captive-portal DNS --------------------------------------------------------

/// Answer every A query with the board's address (others: no answer).
fn dns_server() {
    let sock = match UdpSocket::bind("0.0.0.0:53") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("dns: cannot bind port 53: {e}");
            return;
        }
    };
    let mut buf = [0u8; 512];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut buf) else { continue };
        if n < 12 {
            continue;
        }
        // Question name: labels up to the zero byte, then type and class.
        let mut i = 12;
        while i < n && buf[i] != 0 {
            i += buf[i] as usize + 1;
        }
        if i + 5 > n {
            continue;
        }
        let qtype = u16::from_be_bytes([buf[i + 1], buf[i + 2]]);
        let qend = i + 5;
        let mut r = Vec::with_capacity(qend + 16);
        r.extend_from_slice(&buf[0..2]); // ID
        r.extend_from_slice(&[0x81, 0x80]); // response, recursion available
        r.extend_from_slice(&[0, 1]); // one question
        r.extend_from_slice(&[0, (qtype == 1) as u8, 0, 0, 0, 0]);
        r.extend_from_slice(&buf[12..qend]);
        if qtype == 1 {
            r.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
            r.extend_from_slice(&AP_IP.octets());
        }
        let _ = sock.send_to(&r, peer);
    }
}

// ---- server --------------------------------------------------------------------

pub fn start(coach: SharedCoach) -> anyhow::Result<Web> {
    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    // The access point at 192.168.4.1, handing itself out as DNS server.
    let driver = WifiDriver::new(peripherals.modem, sysloop, None)?;
    let ap = NetifConfiguration {
        ip_configuration: Some(ipv4::Configuration::Router(RouterConfiguration {
            subnet: Subnet { gateway: AP_IP, mask: Mask(24) },
            dhcp_enabled: true,
            dns: Some(AP_IP),
            secondary_dns: None,
        })),
        ..NetifConfiguration::wifi_default_router()
    };
    let mut wifi = EspWifi::wrap_all(driver, EspNetif::new(NetifStack::Sta)?, EspNetif::new_with_conf(&ap)?)?;
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_svc::sys::esp_read_mac(mac.as_mut_ptr(), esp_idf_svc::sys::esp_mac_type_t_ESP_MAC_WIFI_SOFTAP);
    }
    unsafe {
        use esp_idf_svc::sys as sys;
        info!(
            "web: internal RAM free {} KB (largest {} KB) before the server",
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) / 1024,
            sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_INTERNAL) / 1024
        );
    }
    let mut server = EspHttpServer::new(&HttpConfig {
        stack_size: 10 * 1024,
        max_uri_handlers: 8,
        uri_match_wildcard: true,
        ..Default::default()
    })?;
    let json = |req: esp_idf_svc::http::server::Request<&mut esp_idf_svc::http::server::EspHttpConnection>,
                body: &str| {
        let mut resp = req.into_response(
            200,
            None,
            &[("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        )?;
        resp.write_all(body.as_bytes())
    };
    server.fn_handler("/", Method::Get, |req| {
        req.into_response(
            200,
            None,
            &[("Content-Type", "text/html; charset=utf-8"), ("Content-Encoding", "gzip")],
        )?
        .write_all(PAGE_GZ)
    })?;
    let c = coach.clone();
    server.fn_handler("/api/state", Method::Get, move |req| json(req, &state_json(&c)))?;
    let c = coach.clone();
    server.fn_handler("/api/dir", Method::Get, move |req| {
        let rel = param(req.uri(), "p").unwrap_or_default();
        match dir_json(&c, &rel) {
            Some(body) => json(req, &body),
            None => req.into_status_response(400)?.write_all(b"bad path"),
        }
    })?;
    let c = coach.clone();
    server.fn_handler("/api/search", Method::Get, move |req| {
        let q = param(req.uri(), "q").unwrap_or_default();
        json(req, &search_json(&c, &q))
    })?;
    let c = coach.clone();
    server.fn_handler("/api/play", Method::Get, move |req| {
        let ok = param(req.uri(), "p").and_then(|r| safe_rel(&r)).map(|rel| {
            let mut co = c.lock().unwrap();
            let path = co.root.join(rel);
            let ok = path.is_file();
            if ok {
                co.select(&path, true);
            }
            ok
        });
        if ok == Some(true) {
            json(req, "{\"ok\":true}")
        } else {
            req.into_status_response(404)?.write_all(b"no such song")
        }
    })?;
    let c = coach.clone();
    server.fn_handler("/api/cmd", Method::Get, move |req| {
        let cmd = param(req.uri(), "c").unwrap_or_default();
        let v = param(req.uri(), "v");
        if command(&c, &cmd, v) {
            json(req, "{\"ok\":true}")
        } else {
            req.into_status_response(400)?.write_all(b"bad command")
        }
    })?;
    let ssid = format!("PianoCoach-{:02X}{:02X}", mac[4], mac[5]);
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: ssid.as_str().try_into().map_err(|_| anyhow::anyhow!("SSID too long"))?,
        password: PASSWORD.try_into().map_err(|_| anyhow::anyhow!("password too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        channel: 6,
        max_connections: 4,
        ..Default::default()
    }))?;
    // The server is up before Wi-Fi starts: Wi-Fi's start-up briefly
    // fragments the ESP32's internal RAM, and the server task needs a
    // contiguous internal stack.
    wifi.start()?;
    info!("web: Wi-Fi \u{201c}{ssid}\u{201d} (password {PASSWORD}) — open http://192.168.4.1");

    // Anything else (the phones' "is there internet?" probes) goes to the page.
    server.fn_handler("/*", Method::Get, |req| {
        req.into_response(302, Some("Found"), &[("Location", "http://192.168.4.1/")])?
            .write_all(b"")
    })?;
    // Radio sharing stays "balanced": preferring BT starved Wi-Fi (the page
    // took > 8 s); the piano link reconnects by itself if a busy moment
    // drops it.
    crate::threads::spawn_ext("dns", 4 * 1024, dns_server)?;
    Ok(Web { _wifi: wifi, _server: server })
}
