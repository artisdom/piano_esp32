//! Board-independent core of the Tab5 piano player and learning firmware.
//!
//! * [`smf`] — Standard MIDI File parser and tempo map.
//! * [`song`] — timed notes, hand split, chord steps.
//! * [`engine`] — Listen / Learn / Follow state machine producing the MIDI
//!   to send to the GZUT piano.
//! * [`usbmidi`] — USB-MIDI 1.0 event packets.
//! * [`blemidi`] — BLE-MIDI packets in the form the GZUT piano accepts.
//! * [`library`] — sorting and naming of SD-card library entries.
//! * [`search`] — library-wide song search by Latin text, pinyin or initials.

pub mod blemidi;
pub mod engine;
pub mod library;
mod pinyin_table;
pub mod search;
pub mod smf;
pub mod song;
pub mod usbmidi;
