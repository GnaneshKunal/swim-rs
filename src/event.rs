use chrono::{DateTime, Utc};
use std::net::SocketAddr;

pub enum Event<T> {
    Joined(SocketAddr, DateTime<Utc>),
    Left(SocketAddr, DateTime<Utc>),
    Dead(SocketAddr, DateTime<Utc>),
    Data(T),
}
