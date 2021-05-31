use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::membership::MembershipList;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Action {
    Join,
    Joined(SocketAddr, DateTime<Utc>),
    Leave,
    RequestMembershipList,
    MembershipList(MembershipList),
    Ping,
    Pong,
    PingAddress(SocketAddr),
    PongFrom(SocketAddr, DateTime<Utc>),
    DeclaredDead(SocketAddr),
    Dead(SocketAddr),
    NotDead(SocketAddr, DateTime<Utc>),
}
