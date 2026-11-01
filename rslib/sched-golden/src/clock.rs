// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Pins time and OS randomness for the whole process, without any change to the
//! code under test. An executable's definition of a libc symbol wins over
//! libc's own, so std, chrono and the bundled SQLite all see these.
//!
//! Only wall clocks are pinned. Monotonic clocks still run, as thread pools
//! and timeouts need them, and the real kernel clock is never read by anything
//! that can reach the output.

use std::sync::atomic::AtomicI64;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

static NOW_MS: AtomicI64 = AtomicI64::new(0);
static RANDOM_STATE: AtomicU64 = AtomicU64::new(0x5eed_5eed_5eed_5eed);

pub fn set_ms(ms: i64) {
    NOW_MS.store(ms, Ordering::SeqCst);
}

pub fn now_ms() -> i64 {
    NOW_MS.load(Ordering::SeqCst)
}

pub fn now_secs() -> i64 {
    now_ms().div_euclid(1000)
}

pub fn advance_ms(ms: i64) {
    NOW_MS.fetch_add(ms, Ordering::SeqCst);
}

pub fn reset_random(seed: u64) {
    RANDOM_STATE.store(seed, Ordering::SeqCst);
}

/// Stop if the overrides aren't in effect, as the output would be meaningless.
pub fn verify() {
    let observed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64;
    assert_eq!(observed, now_ms(), "the pinned clock isn't being used");
    let symbol = c"getrandom";
    // SAFETY: a valid C string is passed, and the result is only compared.
    let found = unsafe { libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr()) };
    assert_eq!(
        found as usize, getrandom as *const () as usize,
        "the pinned getrandom isn't being used"
    );
}

/// # Safety
/// As for libc's clock_gettime.
#[no_mangle]
pub unsafe extern "C" fn clock_gettime(
    id: libc::clockid_t,
    out: *mut libc::timespec,
) -> libc::c_int {
    if id == libc::CLOCK_REALTIME || id == libc::CLOCK_REALTIME_COARSE {
        let now = now_ms();
        (*out).tv_sec = now.div_euclid(1000) as libc::time_t;
        (*out).tv_nsec = (now.rem_euclid(1000) * 1_000_000) as _;
        0
    } else {
        libc::syscall(libc::SYS_clock_gettime, id, out) as libc::c_int
    }
}

/// # Safety
/// As for libc's gettimeofday.
#[no_mangle]
pub unsafe extern "C" fn gettimeofday(
    out: *mut libc::timeval,
    _timezone: *mut libc::c_void,
) -> libc::c_int {
    let now = now_ms();
    (*out).tv_sec = now.div_euclid(1000) as libc::time_t;
    (*out).tv_usec = (now.rem_euclid(1000) * 1000) as _;
    0
}

/// # Safety
/// As for libc's time.
#[no_mangle]
pub unsafe extern "C" fn time(out: *mut libc::time_t) -> libc::time_t {
    let now = now_secs() as libc::time_t;
    if !out.is_null() {
        *out = now;
    }
    now
}

/// # Safety
/// As for libc's getrandom.
#[no_mangle]
pub unsafe extern "C" fn getrandom(
    buf: *mut libc::c_void,
    len: libc::size_t,
    _flags: libc::c_uint,
) -> libc::ssize_t {
    let out = std::slice::from_raw_parts_mut(buf as *mut u8, len);
    for chunk in out.chunks_mut(8) {
        let state = RANDOM_STATE.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::SeqCst);
        let bytes = crate::json::splitmix(state).to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    len as libc::ssize_t
}
