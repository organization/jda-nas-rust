use std::time::{SystemTime, UNIX_EPOCH};

pub fn timing_get_nano_secs() -> u128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) => n.as_nanos(),
        Err(_) => {
            eprintln!("SystemTime before UNIX EPOCH!");
            0
        }
    }
}
