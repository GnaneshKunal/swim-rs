#![allow(unreachable_code)]

pub mod action;
pub mod event;
pub mod gossip;
pub mod membership;
pub mod message;
pub mod response;
pub mod suspected_node;
pub mod utils;

use std::net::SocketAddr;

use async_std::{
    net::UdpSocket,
    sync::{Arc, RwLock},
};

use message::Message;

pub async fn send_msg(
    socket: Arc<RwLock<UdpSocket>>,
    to_address: SocketAddr,
    msg: Message,
) -> Result<usize, anyhow::Error> {
    let msg_encoded = bincode::serialize(&msg).unwrap();
    let socket = socket.read().await;
    Ok(socket.send_to(&msg_encoded, to_address).await?)
}

pub async fn send_msg_to_nodes(
    socket: Arc<RwLock<UdpSocket>>,
    to_addresses: &[SocketAddr],
    msg: Message,
) -> Result<(), anyhow::Error> {
    for to_address in to_addresses {
        send_msg(socket.clone(), *to_address, msg.clone()).await?;
    }
    Ok(())
}

pub fn bytes_to_msg(msg_encoded: &[u8]) -> Message {
    bincode::deserialize(&msg_encoded).unwrap()
}
