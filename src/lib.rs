#![allow(unreachable_code)]

pub mod action;
pub mod event;
pub mod gossip;
pub mod membership;
pub mod message;
pub mod response;
pub mod suspected_node;
pub mod utils;

use std::{
    marker::{Send, Sync},
    net::SocketAddr,
};

use async_std::{
    net::UdpSocket,
    sync::{Arc, RwLock},
};

use message::Message;

use core::fmt::Debug;
use serde::{Deserialize, Serialize};

pub async fn send_msg<T: Send + Sync + Serialize + Clone + Debug>(
    socket: Arc<RwLock<UdpSocket>>,
    to_address: SocketAddr,
    msg: Message<T>,
) -> Result<usize, anyhow::Error> {
    let msg_encoded = bincode::serialize(&msg).unwrap();
    let socket = socket.read().await;
    Ok(socket.send_to(&msg_encoded, to_address).await?)
}

pub async fn send_msg_to_nodes<'a, T: Send + Sync + Serialize + Clone + Debug>(
    socket: Arc<RwLock<UdpSocket>>,
    to_addresses: &[SocketAddr],
    msg: Message<T>,
) -> Result<(), anyhow::Error> {
    for to_address in to_addresses {
        let msg = msg.clone();
        send_msg(socket.clone(), *to_address, msg).await?;
    }
    Ok(())
}

pub fn bytes_to_msg<'a, T: Sync + Send + Serialize + Deserialize<'a> + Debug + Clone>(
    msg_encoded: &'a [u8],
) -> Message<T> {
    bincode::deserialize(msg_encoded).unwrap()
}
