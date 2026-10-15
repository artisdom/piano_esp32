use std::sync::Arc;

use piano_core::engine::{Engine, Mode, Msg, Options, State, ALL_NOTES_OFF};
use piano_core::library::{self, entry};
use piano_core::smf;
use piano_core::song::{Hand, HandSel, Song};
use piano_core::usbmidi;

fn vlq(mut v: u32, out: &mut Vec<u8>) {
    let mut tmp = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        tmp.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    tmp.reverse();
    out.extend(tmp);
}

/// Build a format-1 SMF: PPQ 480, 120 bpm tempo track, then one track per
/// `(name, [(delta_ticks, bytes)])`.
fn smf_bytes(tracks: &[(&str, Vec<(u32, Vec<u8>)>)]) -> Vec<u8> {
    let mut f = b"MThd".to_vec();
    f.extend(6u32.to_be_bytes());
    f.extend(1u16.to_be_bytes());
    f.extend(((tracks.len() + 1) as u16).to_be_bytes());
    f.extend(480u16.to_be_bytes());
    let mut chunk = |body: Vec<u8>| {
        f.extend(b"MTrk");
        f.extend((body.len() as u32).to_be_bytes());
        f.extend(body);
    };
    // Tempo track: 500000 µs/q = 120 bpm.
    chunk(vec![0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, 0, 0xff, 0x2f, 0]);
    for (name, evs) in tracks {
        let mut b = vec![0, 0xff, 0x03, name.len() as u8];
        b.extend(name.as_bytes());
        for (d, bytes) in evs {
            vlq(*d, &mut b);
            b.extend(bytes);
        }
        b.extend([0, 0xff, 0x2f, 0]);
        chunk(b);
    }
    f
}

/// Right hand: C5 E5 (quarter notes); left hand: C3 held a half note.
fn two_hand_song() -> Song {
    let rh = vec![
        (0, vec![0x90, 72, 80]),
        (480, vec![0x80, 72, 0]),
        (0, vec![0x90, 76, 80]),
        (480, vec![76, 0]), // running status, Note On vel 0 = off
    ];
    let lh = vec![(0, vec![0x90, 48, 60]), (960, vec![0x80, 48, 0])];
    Song::parse(&smf_bytes(&[("Sound 1", rh), ("Sound 2", lh)])).unwrap()
}

#[test]
fn parses_notes_tempo_and_hands() {
    let s = two_hand_song();
    assert_eq!(s.note_tracks, 2);
    assert_eq!(s.notes.len(), 3);
    assert_eq!(s.duration_ms, 1000); // 960 ticks at 120 bpm, PPQ 480
    let c5 = s.notes.iter().find(|n| n.key == 72).unwrap();
    assert_eq!((c5.start_ms, c5.end_ms, c5.hand), (0, 500, Hand::Right));
    let e5 = s.notes.iter().find(|n| n.key == 76).unwrap();
    assert_eq!((e5.start_ms, e5.end_ms), (500, 1000));
    assert_eq!(s.notes.iter().find(|n| n.key == 48).unwrap().hand, Hand::Left);
    assert!((s.bpm - 120.0).abs() < 1e-9);
}

#[test]
fn track_names_override_pitch() {
    // Named left track plays higher than the right one: names win.
    let a = vec![(0, vec![0x90, 40, 80]), (480, vec![0x80, 40, 0])];
    let b = vec![(0, vec![0x90, 80, 80]), (480, vec![0x80, 80, 0])];
    let s = Song::parse(&smf_bytes(&[("Piano right", a), ("Piano left", b)])).unwrap();
    assert_eq!(s.notes.iter().find(|n| n.key == 40).unwrap().hand, Hand::Right);
}

#[test]
fn tempo_change_mid_song() {
    // Tempo track halves the tempo at tick 480 (60 bpm afterwards).
    let mut f = smf_bytes(&[(
        "x",
        vec![(0, vec![0x90, 60, 90]), (960, vec![0x80, 60, 0])],
    )]);
    let p = f.windows(4).position(|w| w == b"MTrk").unwrap();
    let body: Vec<u8> = vec![
        0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, // 120 bpm
        0x83, 0x60, 0xff, 0x51, 3, 0x0f, 0x42, 0x40, // +480: 60 bpm
        0, 0xff, 0x2f, 0,
    ];
    let old_len = u32::from_be_bytes(f[p + 4..p + 8].try_into().unwrap()) as usize;
    f.splice(p + 4..p + 8 + old_len, (body.len() as u32).to_be_bytes().into_iter().chain(body));
    let s = Song::parse(&f).unwrap();
    // 480 ticks @120 = 500 ms, then 480 ticks @60 = 1000 ms.
    assert_eq!(s.notes[0].end_ms, 1500);
}

#[test]
fn chord_steps() {
    let rh = vec![
        (0, vec![0x90, 60, 80]),
        (10, vec![0x90, 64, 80]), // 10 ticks ≈ 10 ms → same chord
        (470, vec![0x80, 60, 0]),
        (0, vec![0x80, 64, 0]),
        (0, vec![0x90, 67, 80]),
        (480, vec![0x80, 67, 0]),
    ];
    let s = Song::parse(&smf_bytes(&[("Piano", rh)])).unwrap();
    let st = s.steps(HandSel::Both);
    assert_eq!(st.len(), 2);
    assert_eq!(st[0].keys, vec![60, 64]);
    assert_eq!(st[1].keys, vec![67]);
}

fn run(e: &mut Engine, from: u64, to: u64, out: &mut Vec<Msg>) {
    let mut t = from;
    while t <= to {
        e.tick(t, out);
        t += 5;
    }
}

#[test]
fn listen_plays_everything_then_finishes() {
    let mut e = Engine::new(Arc::new(two_hand_song()), Options::default());
    let mut out = Vec::new();
    e.play(0, &mut out);
    assert!(out.contains(&[0x90, 72, 80]) && out.contains(&[0x90, 48, 60]));
    out.clear();
    run(&mut e, 0, 1100, &mut out);
    assert!(out.contains(&[0x80, 72, 0]));
    assert!(out.contains(&[0x90, 76, 80]));
    assert!(out.contains(&ALL_NOTES_OFF));
    assert_eq!(e.state(), State::Finished);
}

#[test]
fn learn_waits_for_the_right_hand() {
    let opts = Options {
        mode: Mode::Learn,
        hands: HandSel::Right,
        ..Options::default()
    };
    let mut e = Engine::new(Arc::new(two_hand_song()), opts);
    let mut out = Vec::new();
    e.play(0, &mut out);
    // First chord's LED lit silently; nothing audible yet (waiting at 0).
    assert!(out.contains(&[0x90, 72, 1]));
    assert!(!out.contains(&[0x90, 48, 60]));
    assert_eq!(e.state(), State::Waiting);
    out.clear();
    run(&mut e, 0, 2000, &mut out);
    assert_eq!(e.position_ms(), 0, "clock must not run while waiting");

    // Wrong key counts as wrong and does not advance.
    e.key_down(73, &mut out);
    assert_eq!(e.stats().wrong, 1);
    assert_eq!(e.waiting_keys(), vec![72]);

    // Right key: hint off, next hint (E5) on, accompaniment starts.
    out.clear();
    e.key_down(72, &mut out);
    assert!(out.contains(&[0x80, 72, 0]));
    assert!(out.contains(&[0x90, 76, 1]));
    e.tick(2005, &mut out);
    assert!(out.contains(&[0x90, 48, 60]), "left hand accompanies");

    // Clock runs to E5 at 500 ms and waits again.
    run(&mut e, 2005, 3000, &mut out);
    assert_eq!(e.position_ms(), 500);
    assert_eq!(e.state(), State::Waiting);
    e.key_down(76, &mut out);
    run(&mut e, 3000, 4000, &mut out);
    assert_eq!(e.state(), State::Finished);
    assert_eq!(e.stats().steps_done, 2);
    assert_eq!(e.stats().correct, 2);
}

#[test]
fn learn_easy_mode_any_key_advances() {
    let opts = Options {
        mode: Mode::Learn,
        easy: true,
        ..Options::default()
    };
    let mut e = Engine::new(Arc::new(two_hand_song()), opts);
    let mut out = Vec::new();
    e.play(0, &mut out);
    // Both hands: first step is C3 + C5.
    assert_eq!(e.waiting_keys().len(), 2);
    out.clear();
    e.key_down(30, &mut out);
    assert!(out.contains(&[0x80, 72, 0]) && out.contains(&[0x80, 48, 0]));
    assert_eq!(e.stats().steps_done, 1);
}

#[test]
fn follow_scores_hits_and_misses() {
    let opts = Options {
        mode: Mode::Follow,
        hands: HandSel::Right,
        lead_ms: 400,
        ..Options::default()
    };
    let mut e = Engine::new(Arc::new(two_hand_song()), opts);
    let mut out = Vec::new();
    e.play(0, &mut out);
    assert!(out.contains(&[0x90, 72, 1]), "C5 hint lit ahead");
    e.key_down(72, &mut out); // on time
    run(&mut e, 0, 1200, &mut out); // E5 never played → miss
    assert_eq!(e.stats().hits, 1);
    assert_eq!(e.stats().misses, 1);
    assert!(out.contains(&[0x90, 76, 1]) && out.contains(&[0x80, 76, 0]));
}

#[test]
fn hint_survives_overlapping_accompaniment_off() {
    // LH plays C4 while the RH learner must also press C4 later.
    let rh = vec![(480, vec![0x90, 60, 80]), (480, vec![0x80, 60, 0])];
    let lh = vec![(0, vec![0x90, 60, 70]), (480, vec![0x80, 60, 0])];
    let s = Song::parse(&smf_bytes(&[("Piano right", rh), ("Piano left", lh)])).unwrap();
    let opts = Options {
        mode: Mode::Learn,
        hands: HandSel::Right,
        ..Options::default()
    };
    let mut e = Engine::new(Arc::new(s), opts);
    let mut out = Vec::new();
    e.play(0, &mut out);
    run(&mut e, 0, 1000, &mut out);
    // The LH note-off at 500 ms (= the step) must not leave the hint dark.
    let last_c4 = out.iter().rev().find(|m| m[1] == 60).unwrap();
    assert_eq!(*last_c4, [0x90, 60, 1]);
    assert!(e.is_hint(60));
}

#[test]
fn usb_midi_round_trip() {
    assert_eq!(usbmidi::encode(&[0x90, 60, 1]), Some([0x09, 0x90, 60, 1]));
    assert_eq!(usbmidi::encode(&[0xc0, 5]), Some([0x0c, 0xc0, 5, 0]));
    let sx = [0xf0, 0x52, 0x4b, 0x2a, 0x05, 0x00, 0x00, 0xf7];
    let pk: Vec<u8> = usbmidi::encode_sysex(&sx).concat();
    assert_eq!(pk.len(), 12);
    let mut got = Vec::new();
    let mut d = usbmidi::Decoder::default();
    let mut buf = vec![0x09, 0x90, 0x3c, 0x37, 0x08, 0x80, 0x3c, 0x00];
    buf.extend(&pk);
    buf.extend([0, 0, 0, 0]);
    d.feed(&buf, |m| got.push(m));
    assert_eq!(
        got,
        vec![
            usbmidi::Incoming::Short(vec![0x90, 0x3c, 0x37]),
            usbmidi::Incoming::Short(vec![0x80, 0x3c, 0x00]),
            usbmidi::Incoming::SysEx(sx.to_vec()),
        ]
    );
}

#[test]
fn library_names() {
    let d = entry("小项A：华语流行（897）", true);
    assert_eq!((d.title.as_str(), d.count), ("华语流行", Some(897)));
    let f = entry("洋娃娃之梦 - 奥斯丁.mid", false);
    assert_eq!((f.title.as_str(), f.subtitle.as_str()), ("洋娃娃之梦", "奥斯丁"));
    let v = library::sort_entries(vec![
        entry("b10.mid", false),
        entry("b2.mid", false),
        entry("notes.txt", false),
        entry("z", true),
    ]);
    let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["z", "b2.mid", "b10.mid"]);
    assert_eq!(library::fmt_time(187_000), "3:07");
}

/// Parse every file of the real library when it is available
/// (PIANO_MIDI_DIR, default ../../../midi-piano/assets/midi).
#[test]
fn parses_whole_library() {
    let dir = std::env::var("PIANO_MIDI_DIR").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../../midi-piano/assets/midi").to_string()
    });
    let Ok(_) = std::fs::metadata(&dir) else {
        eprintln!("skip: no library at {dir}");
        return;
    };
    let mut stack = vec![std::path::PathBuf::from(dir)];
    let (mut ok, mut bad, mut empty) = (0, Vec::new(), 0);
    let t0 = std::time::Instant::now();
    while let Some(p) = stack.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if library::is_midi(&e.file_name().to_string_lossy()) {
                match Song::parse(&std::fs::read(&path).unwrap()) {
                    Ok(s) if s.notes.is_empty() => empty += 1,
                    Ok(s) => {
                        assert!(s.duration_ms > 0);
                        assert!(!s.steps(HandSel::Both).is_empty());
                        ok += 1
                    }
                    Err(err) => bad.push(format!("{}: {err}", path.display())),
                }
            }
        }
    }
    eprintln!("library: {ok} ok, {empty} empty, {} failed in {:?}", bad.len(), t0.elapsed());
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(ok > 0);
    let _ = smf::Division::Ppq(1);
}

#[test]
fn ble_midi_packets() {
    use piano_core::blemidi::{decode, Packer};
    // Piano notifications (captured in doc/test_log_2026-10-15.md).
    let mut got: Vec<Vec<u8>> = Vec::new();
    decode(&[0x80, 0x80, 0x90, 0x15, 0x37], |m| got.push(m.to_vec()));
    decode(&[0x80, 0x80, 0x80, 0x15, 0x00], |m| got.push(m.to_vec()));
    decode(
        &[0x80, 0x80, 0xf0, 0x52, 0x4b, 0x15, 0x00, 0x00, 0x00, 0x80, 0xf7],
        |m| got.push(m.to_vec()),
    );
    // Standard multi-message packet: ts+status, ts+running status, bare
    // running status, one-byte real-time.
    decode(
        &[0x80, 0x81, 0x90, 60, 1, 0x82, 64, 2, 67, 3, 0x83, 0xf8, 0x84, 0xc0, 5],
        |m| got.push(m.to_vec()),
    );
    assert_eq!(
        got,
        vec![
            vec![0x90, 0x15, 0x37],
            vec![0x80, 0x15, 0x00],
            vec![0xf0, 0x52, 0x4b, 0x15, 0x00, 0x00, 0x00, 0xf7],
            vec![0x90, 60, 1],
            vec![0x90, 64, 2],
            vec![0x90, 67, 3],
            vec![0xf8],
            vec![0xc0, 5],
        ]
    );

    // Outgoing: one 80 80 header, full status bytes, size-limited.
    let mut p = Packer::new(9);
    assert!(p.push(&[0x90, 60, 1]).is_none());
    assert!(p.push(&[0xc0, 0, 0]).is_none()); // program change is 2 bytes
    let full = p.push(&[0x90, 64, 1]).expect("third message overflows 9 bytes");
    assert_eq!(full, vec![0x80, 0x80, 0x90, 60, 1, 0xc0, 0]);
    assert_eq!(p.flush().unwrap(), vec![0x80, 0x80, 0x90, 64, 1]);
    assert!(p.flush().is_none());
}

#[test]
fn search_by_pinyin_initials_and_latin() {
    use piano_core::search::{keys, Index, SongRef};
    assert_eq!(keys("小燕子"), ("xiaoyanzi".to_string(), "xyz".to_string()));
    assert_eq!(keys("Yesterday Once More").1, "yom");
    let songs = ["小燕子 - 王路.mid", "燕子 - 民歌.mid", "Yesterday Once More - Carpenters.mid", "卖报歌 - 聂耳.mid"]
        .iter()
        .map(|n| SongRef::new(std::path::PathBuf::from(n), "f".into()))
        .collect();
    let idx = Index { songs };
    let titles = |q: &str| idx.search(q, 10).iter().map(|s| s.title.clone()).collect::<Vec<_>>();
    assert_eq!(titles("xyz"), ["小燕子"]);
    assert_eq!(titles("yanzi"), ["燕子", "小燕子"]); // prefix match ranks first
    assert_eq!(titles("Once more"), ["Yesterday Once More"]);
    assert_eq!(titles("nie"), ["卖报歌"]); // by artist pinyin
    assert!(titles("").is_empty());
}

#[test]
fn library_search_index_finds_songs() {
    let dir = std::env::var("PIANO_MIDI_DIR").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../../midi-piano/assets/midi").to_string()
    });
    if std::fs::metadata(&dir).is_err() {
        return;
    }
    let idx = piano_core::search::Index::build(std::path::Path::new(&dir));
    assert_eq!(idx.songs.len(), 2449);
    // 卖报歌 (mai bao ge) by its initials.
    assert!(idx.search("mbg", 20).iter().any(|s| s.title == "卖报歌"));
}

#[test]
fn practice_loop_repeats_section() {
    // RH: C5 at 0 ms, E5 at 500 ms; LH C3 0..1000.
    let mut e = Engine::new(Arc::new(two_hand_song()), Options::default());
    let mut out = Vec::new();
    assert!(!e.set_loop(Some((0, 100)), &mut out), "too short");
    assert!(e.set_loop(Some((0, 600)), &mut out));
    e.play(0, &mut out);
    run(&mut e, 0, 1300, &mut out);
    assert_eq!(e.loops(), 2);
    assert!(e.position_ms() < 600);
    assert_ne!(e.state(), State::Finished);
    e.set_loop(None, &mut out);
    run(&mut e, 1300, 3000, &mut out);
    assert_eq!(e.state(), State::Finished);
}

#[test]
fn learn_loop_restarts_at_section_start() {
    let opts = Options {
        mode: Mode::Learn,
        hands: HandSel::Right,
        ..Options::default()
    };
    let mut e = Engine::new(Arc::new(two_hand_song()), opts);
    let mut out = Vec::new();
    // Section covers only the first chord (C5 at 0); E5 at 500 is past it.
    e.set_loop(Some((0, 500)), &mut out);
    e.play(0, &mut out);
    e.key_down(72, &mut out);
    assert!(!e.is_hint(76), "chord past the loop end is not lit");
    out.clear();
    run(&mut e, 0, 700, &mut out);
    assert_eq!(e.loops(), 1);
    // Back at the start: waiting for C5 again with its LED lit.
    assert_eq!(e.waiting_keys(), vec![72]);
    assert!(out.contains(&[0x90, 72, 1]));
    assert!(!out.contains(&[0x90, 76, 1]), "chord past the loop end is never lit");
    let s = e.song().clone();
    assert_eq!(s.onset_at_or_before(499), 0);
    assert_eq!(s.onset_at_or_before(500), 500);
}
