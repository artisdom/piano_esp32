use std::sync::Arc;

use piano_core::controls::{self, Control};
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

/// Right hand: C5 every 500 ms for 10 s (vel 80); left hand: C3 at the
/// same times (vel 60).
fn steady_song() -> Song {
    let mut rh = Vec::new();
    let mut lh = Vec::new();
    for i in 0..20 {
        let gap = if i == 0 { 0 } else { 240 };
        rh.push((gap, vec![0x90, 72, 80]));
        rh.push((240, vec![0x80, 72, 0]));
        lh.push((gap, vec![0x90, 48, 60]));
        lh.push((240, vec![0x80, 48, 0]));
    }
    Song::parse(&smf_bytes(&[("Piano right", rh), ("Piano left", lh)])).unwrap()
}

/// Velocities of the Note Ons sent for `key` (hints excluded).
fn struck(out: &[Msg], key: u8) -> Vec<u8> {
    out.iter().filter(|m| m[0] == 0x90 && m[1] == key && m[2] > 1).map(|m| m[2]).collect()
}

fn follow(hands: HandSel) -> Options {
    Options {
        mode: Mode::Follow,
        hands,
        lead_ms: 400,
        ..Options::default()
    }
}

#[test]
fn follow_plays_like_listen_until_you_join_in() {
    let mut e = Engine::new(Arc::new(two_hand_song()), follow(HandSel::Both));
    let mut out = Vec::new();
    e.play(0, &mut out);
    run(&mut e, 0, 1200, &mut out);
    for m in [[0x90, 72, 80], [0x90, 76, 80], [0x90, 48, 60]] {
        assert!(out.contains(&m), "{m:?} at full velocity");
    }
    assert!(out.contains(&[0x90, 76, 1]), "keys still lit ahead");
    assert_eq!((e.stats().hits, e.stats().misses, e.stats().wrong), (0, 0, 0), "listening is not scored");
}

#[test]
fn follow_plays_the_hand_you_play_softly() {
    let mut e = Engine::new(Arc::new(steady_song()), follow(HandSel::Both));
    let mut out = Vec::new();
    e.play(0, &mut out);
    e.key_down(72, &mut out); // right hand joins on the first note
    let mut t = 0;
    while t < 2000 {
        t += 5;
        e.tick(t, &mut out);
        if t % 500 == 0 {
            e.key_down(72, &mut out);
        }
    }
    assert_eq!(e.following(), (true, false));
    let rh = struck(&out, 72);
    // The first C5 sounded before the learner's first hit; then soft.
    assert_eq!(rh[0], 80);
    assert!(rh[1..].iter().all(|&v| v == 32), "right hand guided softly: {rh:?}");
    let lh = struck(&out, 48);
    assert!(lh.iter().all(|&v| v == 60), "left hand at full velocity: {lh:?}");
    assert_eq!(e.stats().misses, 0);
}

#[test]
fn follow_returns_to_listen_after_four_idle_seconds() {
    let mut e = Engine::new(Arc::new(steady_song()), follow(HandSel::Both));
    let mut out = Vec::new();
    e.play(0, &mut out);
    e.key_down(72, &mut out); // one hit, then nothing
    run(&mut e, 0, 9000, &mut out);
    assert_eq!(e.following(), (false, false));
    let rh = struck(&out, 72);
    // C5 at 0 s full (before the hit), 0.5 … 3.5 s soft, 4 … 9 s full again.
    assert_eq!(rh.len(), 19);
    assert!(rh[1..8].iter().all(|&v| v == 32), "{rh:?}");
    assert!(rh[8..].iter().all(|&v| v == 80), "{rh:?}");
    // Misses count only while the hand was being played.
    assert_eq!(e.stats().misses, 7);
}

#[test]
fn follow_hands_setting_picks_the_tracked_hand() {
    let mut e = Engine::new(Arc::new(steady_song()), follow(HandSel::Right));
    let mut out = Vec::new();
    e.play(0, &mut out);
    e.key_down(48, &mut out); // left hand is not tracked: not a hit
    e.key_down(72, &mut out);
    run(&mut e, 0, 2000, &mut out);
    assert_eq!(e.following(), (true, false));
    assert!(struck(&out, 48).iter().all(|&v| v == 60), "untracked hand always at full velocity");
    assert!(!out.contains(&[0x90, 48, 1]), "untracked hand gets no hints");
    assert_eq!(e.stats().hits, 1);
}

#[test]
fn follow_skips_notes_played_early() {
    let mut e = Engine::new(Arc::new(two_hand_song()), follow(HandSel::Right));
    let mut out = Vec::new();
    e.play(0, &mut out);
    run(&mut e, 0, 300, &mut out);
    e.key_down(76, &mut out); // E5 (at 500 ms) played 200 ms early
    run(&mut e, 305, 1200, &mut out);
    assert_eq!((e.stats().hits, e.stats().misses), (1, 0));
    assert!(out.contains(&[0x90, 72, 80]), "C5 played by the piano before you joined");
    assert!(struck(&out, 76).is_empty(), "E5 not doubled");
    assert!(out.contains(&[0x90, 48, 60]), "left hand at full velocity");
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

fn goertzel(x: &[i16], rate: f32, f: f32) -> f32 {
    let w = 2.0 * std::f32::consts::PI * f / rate;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0f32, 0f32);
    for &v in x {
        let s0 = v as f32 + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - c * s1 * s2
}

fn rms(x: &[i16]) -> f32 {
    (x.iter().map(|&v| (v as f32).powi(2)).sum::<f32>() / x.len() as f32).sqrt()
}

#[test]
fn synth_pitch_decay_damper_sustain() {
    use piano_core::synth::{Synth, MAX_VOICES};
    let rate = 32_000;
    let mut s = Synth::new(rate);
    s.note_on(69, 100); // A4
    let mut buf = vec![0i16; rate as usize / 2];
    s.render(&mut buf);
    assert!(rms(&buf) > 1000.0, "audible");
    assert!(goertzel(&buf, rate as f32, 440.0) > 20.0 * goertzel(&buf, rate as f32, 466.2));

    // Held note decays on its own and frees the voice eventually.
    let mut long = vec![0i16; rate as usize * 40];
    s.render(&mut long);
    assert_eq!(s.active_voices(), 0);

    // Note off damps within ~1 s; with the pedal down it keeps ringing.
    let mut a = Synth::new(rate);
    a.note_on(60, 100);
    a.note_off(60);
    let mut b = Synth::new(rate);
    b.set_sustain(true);
    b.note_on(60, 100);
    b.note_off(60);
    let (mut ba, mut bb) = (vec![0i16; rate as usize], vec![0i16; rate as usize]);
    a.render(&mut ba);
    b.render(&mut bb);
    let tail = rate as usize * 3 / 4..;
    assert!(rms(&bb[tail.clone()]) > 10.0 * rms(&ba[tail.clone()]).max(1.0));
    b.set_sustain(false);
    b.render(&mut bb);
    assert!(rms(&bb[tail]) < 50.0, "pedal up damps");

    // Polyphony is bounded; CC 123 silences everything.
    let mut p = Synth::new(rate);
    for k in 30..90 {
        p.midi(&[0x90, k, 90]);
    }
    assert_eq!(p.active_voices(), MAX_VOICES);
    p.midi(&[0xb0, 123, 0]);
    assert_eq!(p.active_voices(), 0);
}

#[test]
fn synth_render_speed() {
    // 24 voices for 2 s of audio; prints the real-time factor on the host.
    let rate = 32_000;
    let mut s = piano_core::synth::Synth::new(rate);
    for k in 40..64 {
        s.note_on(k, 100);
    }
    let mut buf = vec![0i16; rate as usize * 2];
    let t = std::time::Instant::now();
    s.render(&mut buf);
    let el = t.elapsed().as_secs_f32();
    eprintln!("synth: 24 voices x 2 s rendered in {:.1} ms ({:.0}x real time)", el * 1e3, 2.0 / el);
}

#[test]
fn progress_bookmarks_scores_recent_roundtrip() {
    use piano_core::progress::{stars, star_text, Progress, ScoreKind};
    let mut p = Progress::default();
    assert!(p.toggle_bookmark("a/卖报歌 - 聂耳.mid"));
    p.record_play("a/卖报歌 - 聂耳.mid");
    p.record_play("b/x.mid");
    assert!(p.record_score("b/x.mid", ScoreKind::Learn, 70));
    assert!(!p.record_score("b/x.mid", ScoreKind::Learn, 65), "not a new best");
    assert!(p.record_score("b/x.mid", ScoreKind::Learn, 96));
    assert!(p.record_score("b/x.mid", ScoreKind::Follow, 50));
    let q = Progress::parse(&p.to_text());
    assert_eq!(q.bookmarks(), vec!["a/卖报歌 - 聂耳.mid"]);
    assert_eq!(q.recent(5), vec!["b/x.mid", "a/卖报歌 - 聂耳.mid"]);
    let r = q.get("b/x.mid");
    assert_eq!((r.best_learn, r.best_follow, r.plays), (Some(96), Some(50), 1));
    // Un-bookmarking a song with no other data removes its line.
    let mut q = q;
    q.toggle_bookmark("c.mid");
    q.toggle_bookmark("c.mid");
    assert!(!q.to_text().contains("c.mid"));
    // Play sequence continues after a reload.
    q.record_play("c.mid");
    assert_eq!(q.recent(1), vec!["c.mid"]);
    assert_eq!((stars(59), stars(60), stars(80), stars(95)), (0, 1, 2, 3));
    assert_eq!(star_text(Some(92)), "★★☆ 92%");
}

#[test]
fn sphere_pick_face_and_render() {
    use piano_core::sphere::Sphere;
    let n = 500;
    let mut s = Sphere::new((0..n).map(|i| (i * 5 / n) as u8).collect());
    let (w, h) = (400, 300);
    // Turning song 123 to the front puts it at the centre, where a tap picks it.
    s.face(123);
    let p = s.project(123, w, h);
    assert!(p.z > 0.99, "faces the viewer");
    assert!((p.x - 200.0).abs() < 1.0 && (p.y - 150.0).abs() < 1.0);
    assert_eq!(s.pick(200.0, 150.0, w, h, 20.0), Some(123));
    assert_eq!(s.pick(5.0, 5.0, w, h, 20.0), None, "corner is empty space");
    // Drag spins it; inertia keeps it moving after release, then settles.
    let yaw0 = s.yaw;
    s.drag(50.0, 0.0, 16.0);
    s.release();
    assert!(s.yaw > yaw0);
    assert!(s.step(16.0));
    for _ in 0..400 {
        s.step(16.0);
    }
    // Render draws the selected ring and coloured dots.
    let mut buf = vec![0u8; w * h * 3];
    let fav = vec![false; n];
    s.face(10);
    s.render(&mut buf, w, h, &fav, Some(10));
    let centre = &buf[(150 * w + 200) * 3..][..3];
    assert_ne!(centre, [0x16, 0x1b, 0x22], "something drawn at the centre");
}

#[test]
fn sphere_tiled_drawing_matches_full() {
    use piano_core::sphere::{RgbCanvas, Sphere};
    let n = 800;
    let mut s = Sphere::new((0..n).map(|i| (i * 3 / n) as u8).collect());
    s.face(100);
    let (w, h) = (300, 200);
    let fav: Vec<bool> = (0..n).map(|i| i % 50 == 0).collect();
    let mut full = vec![0u8; w * h * 3];
    s.draw_dots(&mut RgbCanvas { buf: &mut full, w }, w, h, &fav, Some(100));
    let mut tiled = vec![0u8; w * h * 3];
    let order = s.depth_order(w, h);
    for x0 in (0..w).step_by(32) {
        s.draw_dots_in(&mut RgbCanvas { buf: &mut tiled, w }, w, h, &order, &fav, Some(100), x0..x0 + 32);
    }
    assert!(full.iter().any(|&b| b != 0));
    assert_eq!(full, tiled);
}

#[test]
fn songpack_round_trip() {
    use piano_core::songpack::{build, Input, Kind, Pack, PackError};
    let midi = |n: u8| smf_bytes(&[("Piano", vec![(0, vec![0x90, 60 + n, 80]), (480, vec![0x80, 60 + n, 0])])]);
    let inputs = vec![
        Input { path: "B（2）/b10 - x.mid".into(), midi: midi(2), duration_ms: 500 },
        Input { path: "B（2）/b2 - y.mid".into(), midi: midi(3), duration_ms: 500 },
        Input { path: "A/sub/deep.mid".into(), midi: midi(4), duration_ms: 500 },
        Input { path: "top.mid".into(), midi: midi(1), duration_ms: 500 },
    ];
    let bytes = build(&inputs);
    let p = Pack::new(&bytes).unwrap();
    let root = p.root();
    assert_eq!(root.songs(), 4);
    // Folders first (natural order), then songs.
    let kids: Vec<_> = root.children().map(|i| p.node(i)).collect();
    assert_eq!(kids.iter().map(|n| n.name).collect::<Vec<_>>(), ["A", "B（2）", "top.mid"]);
    assert_eq!(kids[2].kind, Kind::Song);
    let b: Vec<_> = kids[1].children().map(|i| p.node(i).name).collect();
    assert_eq!(b, ["b2 - y.mid", "b10 - x.mid"]);
    // Every song inflates back to its file and knows its path.
    for i in p.songs() {
        let path = p.path(i);
        let src = inputs.iter().find(|s| s.path == path).expect("path round-trips");
        assert_eq!(p.midi(i).unwrap(), src.midi);
        assert!(Song::parse(&p.midi(i).unwrap()).is_ok());
    }
    assert_eq!(p.songs().len(), 4);
    // A trailing partition tail is fine; a damaged index is not.
    let mut padded = bytes.clone();
    padded.extend([0xffu8; 100]);
    assert!(Pack::new(&padded).is_ok());
    let mut bad = bytes.clone();
    bad[40] ^= 1;
    assert_eq!(Pack::new(&bad).unwrap_err(), PackError::Crc);
    assert_eq!(Pack::new(&[0xff; 64]).unwrap_err(), PackError::BadMagic);
}

/// The pack's in-place decompressor must decode every real song exactly.
#[test]
fn songpack_inflates_library() {
    use piano_core::songpack::{build, Input, Pack};
    let dir = std::env::var("PIANO_MIDI_DIR")
        .unwrap_or_else(|_| format!("{}/w/piano/midi-piano/assets/midi", std::env::var("HOME").unwrap()));
    let mut inputs = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(&dir)];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if library::is_midi(&p.to_string_lossy()) && inputs.len() < 300 {
                let rel = p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned();
                inputs.push(Input { path: rel, midi: std::fs::read(&p).unwrap(), duration_ms: 0 });
            }
        }
    }
    if inputs.is_empty() {
        eprintln!("no library at {dir}; skipped");
        return;
    }
    let bytes = build(&inputs);
    let p = Pack::new(&bytes).unwrap();
    for i in p.songs() {
        let src = inputs.iter().find(|s| s.path == p.path(i)).unwrap();
        assert_eq!(p.midi(i).unwrap(), src.midi, "{}", src.path);
    }
}

#[test]
fn notation_engraves_a_melody() {
    use piano_core::notation::{engrave, Glyph, Params, Shape};
    // C4 D4 E4 F4 quarters, then G4 half + half rest: two 4/4 measures.
    let mut ev = Vec::new();
    for (i, k) in [60u8, 62, 64, 65].iter().enumerate() {
        ev.push((if i == 0 { 0 } else { 0 }, vec![0x90, *k, 80]));
        ev.push((480, vec![0x80, *k, 0]));
    }
    ev.push((0, vec![0x90, 67, 80]));
    ev.push((960, vec![0x80, 67, 0]));
    ev.push((960, vec![0xB0, 1, 0])); // pad the track to the bar's end
    let song = Song::parse(&smf_bytes(&[("Right", ev)])).unwrap();
    assert_eq!(song.meter.key, 0);
    let score = engrave(&song, Params { width: 1200.0, space: 10.0 });
    assert_eq!(score.systems.len(), 1);
    let items = &score.systems[0].items;
    let heads: Vec<_> = items
        .iter()
        .filter_map(|i| match i.shape {
            Shape::Glyph { g: g @ (Glyph::HeadBlack | Glyph::HeadHalf), .. } => Some((g, i.note.unwrap().1)),
            _ => None,
        })
        .collect();
    assert_eq!(
        heads,
        [(Glyph::HeadBlack, 60), (Glyph::HeadBlack, 62), (Glyph::HeadBlack, 64), (Glyph::HeadBlack, 65), (Glyph::HeadHalf, 67)]
    );
    // Middle C gets one ledger line; no accidentals in C major.
    assert!(!items.iter().any(|i| matches!(i.shape, Shape::Glyph { g: Glyph::Sharp | Glyph::Flat | Glyph::Natural, .. })));
    // The empty bass staff shows whole-measure rests; the treble a half rest.
    let rests = |g| items.iter().filter(|i| matches!(i.shape, Shape::Glyph { g: x, .. } if x == g)).count();
    assert_eq!(rests(Glyph::RestWhole), 2);
    assert_eq!(rests(Glyph::RestHalf), 1);
    // The cursor walks left to right.
    let a = score.locate(0).unwrap();
    let b = score.locate(1500).unwrap();
    assert_eq!(a.0, 0);
    assert!(b.1 > a.1);
}

#[test]
fn key_estimate_prefers_fewest_accidentals() {
    use piano_core::song::estimate_key;
    let notes = |keys: &[u8]| -> Vec<piano_core::song::Note> {
        keys.iter()
            .enumerate()
            .map(|(i, &key)| piano_core::song::Note {
                start_ms: i as u32 * 100,
                end_ms: i as u32 * 100 + 90,
                key,
                vel: 80,
                track: 0,
                hand: Hand::Right,
            })
            .collect()
    };
    // D major scale: two sharps.
    assert_eq!(estimate_key(&notes(&[62, 64, 66, 67, 69, 71, 73, 74])), 2);
    // F major: one flat.
    assert_eq!(estimate_key(&notes(&[65, 67, 69, 70, 72, 74, 76, 77])), -1);
    // C major.
    assert_eq!(estimate_key(&notes(&[60, 62, 64, 65, 67, 69, 71, 72])), 0);
}

/// Every song of the library engraves; every note gets exactly one head.
#[test]
fn notation_engraves_library() {
    use piano_core::notation::{engrave, Params};
    let dir = std::env::var("PIANO_MIDI_DIR")
        .unwrap_or_else(|_| format!("{}/w/piano/midi-piano/assets/midi", std::env::var("HOME").unwrap()));
    let mut stack = vec![std::path::PathBuf::from(&dir)];
    let (mut n, mut systems) = (0, 0);
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Ok(song) = Song::parse(&std::fs::read(&p).unwrap()) else { continue };
            let score = engrave(&song, Params { width: 1212.0, space: 9.0 });
            // Same key in the same 16th collapses into one head.
            let q16 = |ms: u32| (song.meter.q_at(ms as f64) * 4.0).round() as i64;
            let heads: std::collections::BTreeSet<(i64, u8)> = score
                .systems
                .iter()
                .flat_map(|s| s.items.iter().filter_map(|i| i.note))
                .map(|(ms, k)| (q16(ms), k))
                .collect();
            let want: std::collections::BTreeSet<(i64, u8)> = song.notes.iter().map(|n| (q16(n.start_ms), n.key)).collect();
            assert_eq!(heads, want, "{}", p.display());
            n += 1;
            systems += score.systems.len();
        }
    }
    eprintln!("engraved {n} songs into {systems} systems");
}

#[test]
fn control_keys_map_and_yield_to_nearby_notes() {
    assert_eq!(controls::control(21), Some(Control::PrevSong));
    assert_eq!(controls::control(105), Some(Control::Mode(Mode::Follow)));
    assert_eq!(controls::control(60), None);
    assert_eq!(controls::key_of(Control::Bookmark), Some(108));
    // Keys are unique and inside the 88 (A0 21 … C8 108).
    for (i, (k, c)) in controls::KEYS.iter().enumerate() {
        assert!((21..=108).contains(k));
        assert!(controls::KEYS[i + 1..].iter().all(|o| o.0 != *k && o.1 != *c));
    }
    // A left-hand part with A0 at the start and F1 much later: mid-song a
    // control key is a note only near a note of that key, in a hand the
    // learner plays; when nothing plays it is always a control.
    let lh = vec![
        (0, vec![0x90, 21, 60]),
        (480, vec![0x80, 21, 0]),
        (9600, vec![0x90, 29, 60]),
        (480, vec![0x80, 29, 0]),
    ];
    let s = Song::parse(&smf_bytes(&[("Piano left", lh)])).unwrap();
    let f1 = s.notes.iter().find(|n| n.key == 29).unwrap();
    assert_eq!(f1.hand, Hand::Left);
    let f1_ms = f1.start_ms;
    assert!(f1_ms >= controls::NOTE_BEFORE_MS + controls::NOTE_AFTER_MS + 1000);
    let at = |pos: u32, hands: HandSel| Some((&s, pos, hands));
    assert_eq!(controls::press(21, None), Some(Control::PrevSong));
    assert_eq!(controls::press(21, at(0, HandSel::Both)), None);
    assert_eq!(controls::press(21, at(controls::NOTE_BEFORE_MS + 1, HandSel::Both)), Some(Control::PrevSong));
    assert_eq!(controls::press(21, at(0, HandSel::Right)), Some(Control::PrevSong));
    // F1 is far ahead at the start, a note once it comes near.
    assert_eq!(controls::press(29, at(0, HandSel::Both)), Some(Control::NextCategory));
    let near = f1_ms - controls::NOTE_AFTER_MS + 500;
    assert_eq!(controls::press(29, at(near, HandSel::Left)), None);
    assert_eq!(controls::note_near(&s, 29, near, HandSel::Left), Some((controls::NOTE_AFTER_MS - 500) as i64));
    assert_eq!(controls::press(29, at(f1_ms + controls::NOTE_BEFORE_MS + 1, HandSel::Both)), Some(Control::NextCategory));
    // A key the song never plays, and a non-control key.
    assert_eq!(controls::press(23, at(0, HandSel::Both)), Some(Control::NextSong));
    assert_eq!(controls::press(60, None), None);
}

#[test]
fn shuffle_plays_each_song_once_starting_with_the_current() {
    let songs: Vec<String> = (0..10).map(|i| format!("s{i}.mid")).collect();
    let mut sh = controls::Shuffle::new(songs.clone(), Some("s7.mid"), 42);
    assert_eq!(sh.len(), 10);
    assert_eq!(sh.current(), Some("s7.mid"));
    let mut seen = vec![sh.current().unwrap().to_string()];
    while let Some(s) = sh.step(1) {
        seen.push(s.to_string());
    }
    let mut sorted = seen.clone();
    sorted.sort();
    assert_eq!(sorted, songs, "every song exactly once");
    assert_eq!(sh.played(), 10);
    // Another seed, another order (same songs).
    let other = controls::Shuffle::new(songs.clone(), None, 7);
    let mut o = vec![other.current().unwrap().to_string()];
    let mut other = other;
    while let Some(s) = other.step(1) {
        o.push(s.to_string());
    }
    assert_ne!(o, seen);
    // Back never goes before the first song.
    let mut b = controls::Shuffle::new(songs, None, 1);
    let first = b.current().unwrap().to_string();
    assert_eq!(b.step(-1).map(str::to_string), Some(first));
    assert_eq!(controls::control(32), Some(Control::ShuffleFolder));
}

#[test]
fn timeline_entries_stay_small() {
    // A 1200-note song in Follow keeps 4800 of them on boards without PSRAM.
    assert!(piano_core::engine::timed_size() <= 8);
}
