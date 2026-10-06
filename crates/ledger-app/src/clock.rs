//! Today, for the one figure that depends on it. UTC rather than local:
//! `time` will not read a local offset from a threaded process.

/// The year, and how many whole months of it have gone (January is none).
pub fn year_and_month() -> (i32, u32) {
    let now = time::OffsetDateTime::now_utc();
    (now.year(), u8::from(now.month()) as u32 - 1)
}
