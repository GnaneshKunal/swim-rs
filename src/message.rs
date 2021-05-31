use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::action::Action;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub action: Action,
}

impl Message {
    pub fn new(action: Action) -> Self {
        Self {
            timestamp: Utc::now(),
            action,
        }
    }
}
