use serde::Serialize;
use std::net::SocketAddr;

use crate::message::Message;

use std::fmt::Debug;

#[derive(Debug, Serialize, Clone)]
pub struct Response<T: Sync + Send + Serialize + Clone + Debug> {
    pub address: SocketAddr,
    pub message: Message<T>,
}

impl<T: Sync + Send + Serialize + Clone + Debug> Response<T> {
    pub fn new(address: SocketAddr, message: Message<T>) -> Self {
        Self { address, message }
    }
}
