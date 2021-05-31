use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::action::Action;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message<T: 'static + Serialize + Clone + std::marker::Send + std::marker::Sync> {
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub action: Action<T>,
}

impl<T: Serialize + Clone + std::marker::Send + std::marker::Sync> Message<T> {
    pub fn new(action: Action<T>) -> Self {
        Self {
            timestamp: Utc::now(),
            action,
        }
    }
}
