//! Persistent settings in the default NVS partition (namespace "piano").

use std::sync::Mutex;

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use log::warn;

static NVS: Mutex<Option<EspNvs<NvsDefault>>> = Mutex::new(None);

pub fn init() {
    match EspDefaultNvsPartition::take().and_then(|p| EspNvs::new(p, "piano", true)) {
        Ok(nvs) => *NVS.lock().unwrap() = Some(nvs),
        Err(e) => warn!("prefs: NVS unavailable ({e}); settings will not persist"),
    }
}

pub fn get_i32(key: &str) -> Option<i32> {
    NVS.lock().unwrap().as_ref()?.get_i32(key).ok().flatten()
}

pub fn set_i32(key: &str, v: i32) {
    if let Some(nvs) = NVS.lock().unwrap().as_mut() {
        if let Err(e) = nvs.set_i32(key, v) {
            warn!("prefs: save {key}: {e}");
        }
    }
}

pub fn get_bool(key: &str, default: bool) -> bool {
    get_i32(key).map(|v| v != 0).unwrap_or(default)
}

pub fn set_bool(key: &str, v: bool) {
    set_i32(key, v as i32);
}
