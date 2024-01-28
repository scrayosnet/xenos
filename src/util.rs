use std::time::{SystemTime, UNIX_EPOCH};

pub fn get_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn has_elapsed(time: &u64, dur: &u64) -> bool {
    let now = get_epoch_seconds();
    time + dur < now
}
