use chrono::{DateTime, Utc};
use std::net::SocketAddr;

pub enum Event {
    Joined(SocketAddr, DateTime<Utc>),
    Left(SocketAddr, DateTime<Utc>),
    Dead(SocketAddr, DateTime<Utc>),
}
