use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::action::Action;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message<T: Sync + Send + Serialize + Clone + Debug> {
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub action: Action<T>,
}

impl<'a, T: Sync + Send + Serialize + Deserialize<'a> + Clone + Debug> Message<T> {
    pub fn new(action: Action<T>) -> Self {
        Self {
            timestamp: Utc::now(),
            action,
        }
    }
}
