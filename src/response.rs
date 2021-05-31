use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::message::Message;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    pub address: SocketAddr,
    pub message: Message,
}

impl Response {
    pub fn new(address: SocketAddr, message: Message) -> Self {
        Self { address, message }
    }
}
