use chrono::{DateTime, Utc};

pub fn time_difference_now(time: &DateTime<Utc>) -> i64 {
    (Utc::now() - *time).num_seconds()
}
