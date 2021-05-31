use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::message::Message;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response<T: 'static + Serialize + Clone + std::marker::Send + std::marker::Sync> {
    pub address: SocketAddr,
    pub message: Message<T>,
}

impl<T: Serialize + Clone + std::marker::Send + std::marker::Sync> Response<T> {
    pub fn new(address: SocketAddr, message: Message<T>) -> Self {
        Self { address, message }
    }
}
