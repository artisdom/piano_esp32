use core::ffi;

// TODO: Figure out which library references this
// ESP-IDF 6.2's picolibc supplies timegm; defining the fallback there makes
// a full application link fail with a duplicate symbol.
#[cfg(not(esp_idf_version_at_least_6_2_0))]
#[no_mangle]
pub extern "C" fn timegm(_: ffi::c_void) -> ffi::c_int {
    // Not supported but don't crash just in case
    0
}

// Called by the rand crate
#[no_mangle]
pub extern "C" fn pthread_atfork(
    _: *const ffi::c_void,
    _: *const ffi::c_void,
    _: *const ffi::c_void,
) -> ffi::c_int {
    0
}
