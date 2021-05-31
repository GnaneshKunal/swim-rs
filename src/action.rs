use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    marker::{Send, Sync},
    net::SocketAddr,
};

use crate::{membership::MembershipList, GenericMsgTrait};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Action<T: 'static + Serialize + Send + Sync> {
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
    Data(T),
}
