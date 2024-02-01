use chrono::Utc;

pub fn get_epoch_seconds() -> u64 {
    u64::try_from(Utc::now().timestamp()).unwrap()
}

pub fn has_elapsed(time: &u64, dur: &u64) -> bool {
    let now = get_epoch_seconds();
    time + dur < now
}
