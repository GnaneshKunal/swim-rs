use async_std::{
    channel::{unbounded, Receiver, Sender},
    future,
    net::UdpSocket,
    sync::{Arc, RwLock},
    task,
};
use chrono::Utc;
use log::{debug, error};
use rand::{seq::IteratorRandom, thread_rng};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use crate::{
    action::Action, bytes_to_msg, event::Event, membership::MembershipList, message::Message,
    response::Response, send_msg, send_msg_to_nodes, suspected_node::SuspectedNodeList, utils,
};

#[derive(Debug)]
pub struct GossipInner {
    pub members: MembershipList,
    pub suspected_nodes: SuspectedNodeList,
}

impl GossipInner {
    pub fn get_random_nodes(&self, filter_addresses: &[SocketAddr]) -> Vec<SocketAddr> {
        let mut rng = thread_rng();
        self.members
            .keys()
            .filter(|k| !filter_addresses.contains(k))
            .choose_multiple(&mut rng, 2)
            .iter()
            .map(|&&k| k)
            .collect()
    }

    pub fn process_membership_list(
        &mut self,
        membership_list: MembershipList,
    ) -> Result<(), anyhow::Error> {
        for (node, node_timestamp) in membership_list.iter() {
            if self.members.contains_key(node) {
                let timestamp = self.members.get(node).unwrap().clone();

                let new_timestamp = std::cmp::max(timestamp, *node_timestamp);

                self.members.insert(*node, new_timestamp);
            } else {
                let timestamp_difference = utils::time_difference_now(node_timestamp);
                if timestamp_difference < 30 {
                    debug!("Adding new node {:?}", node);
                    self.members.insert(*node, *node_timestamp);
                } else {
                    debug!("Tried to add an expired node {:?}", node);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Gossip {
    pub address: SocketAddr,
    pub inner: Arc<RwLock<GossipInner>>,
}

impl Gossip {
    pub fn new<A: ToSocketAddrs>(address: A, seed_nodes: &[A]) -> Self {
        let address = address.to_socket_addrs().unwrap().next().unwrap();

        let seed_nodes: Vec<SocketAddr> = seed_nodes
            .iter()
            .map(|n| n.to_socket_addrs().unwrap().next().unwrap())
            .collect();

        let members = MembershipList::new(&seed_nodes);
        let gossip_inner = GossipInner {
            members,
            suspected_nodes: SuspectedNodeList::new(),
        };
        Self {
            address,
            inner: Arc::new(RwLock::new(gossip_inner)),
        }
    }

    pub async fn get_random_nodes(&self) -> Vec<SocketAddr> {
        self.inner.read().await.get_random_nodes(&[self.address])
    }

    pub async fn start(&self, event_sender: Sender<Event>) -> Result<(), anyhow::Error> {
        let (sx, rx) = unbounded();
        // let (event_sender, event_receiver) = unbounded();

        let gossip_node = self;
        let socket: Arc<RwLock<UdpSocket>> =
            Arc::new(RwLock::new(UdpSocket::bind(self.address).await?));

        println!("HERE before");

        // `try_join` is better than `join`
        futures::try_join!(
            gossip_node.transmitter(socket.clone()),
            gossip_node.receiver(socket.clone(), sx),
            gossip_node.processor(socket.clone(), rx, event_sender),
            gossip_node.checker(socket.clone()),
            gossip_node.membership_syncer(socket.clone())
        )?;

        println!("HERE");
        Ok(())
        // Ok(event_receiver)
    }

    pub async fn receiver(
        &self,
        socket: Arc<RwLock<UdpSocket>>,
        sx: Sender<Response>,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let socket = socket.clone();

            debug!("Starting receiver...");

            async move {
                let mut buf = vec![0u8; 1024];

                loop {
                    let response = async_std::future::timeout(
                        Duration::from_millis(5),
                        socket.write().await.recv_from(&mut buf),
                    )
                    .await;

                    match response {
                        Ok(response) => match response {
                            Ok((_, peer)) => {
                                let msg = bytes_to_msg(&buf);

                                // send to inbox
                                sx.send(Response::new(peer, msg)).await?;
                            }
                            _ => (),
                        },
                        _ => task::sleep(Duration::from_secs(1)).await,
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn checker(&self, socket: Arc<RwLock<UdpSocket>>) -> Result<(), anyhow::Error> {
        task::spawn({
            let socket = socket.clone();
            let gossip_node = self.inner.clone();
            let address = self.address.clone();

            debug!("Starting checker...");

            async move {
                task::sleep(Duration::from_secs(1)).await;

                loop {
                    let nodes = gossip_node.read().await.get_random_nodes(&[address]);

                    for node in nodes {
                        let node_timestamp_difference = utils::time_difference_now(
                            gossip_node.read().await.members.get(&node).unwrap(),
                        );
                        if node_timestamp_difference > 30 {
                            send_msg_to_nodes(
                                socket.clone(),
                                &gossip_node.read().await.get_random_nodes(&[node]),
                                Message::new(Action::Dead(node)),
                            )
                            .await?;
                        } else if node_timestamp_difference > 20 {
                            send_msg_to_nodes(
                                socket.clone(),
                                &gossip_node.read().await.get_random_nodes(&[node]),
                                Message::new(Action::DeclaredDead(node)),
                            )
                            .await?;
                        }
                    }

                    task::sleep(Duration::from_secs(5)).await;
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn process_membership_list(
        &self,
        membership_list: MembershipList,
    ) -> Result<(), anyhow::Error> {
        self.inner
            .write()
            .await
            .process_membership_list(membership_list)
    }

    pub async fn get_members(&self) -> MembershipList {
        self.inner.read().await.members.clone()
    }

    pub async fn transmitter(&self, socket: Arc<RwLock<UdpSocket>>) -> Result<(), anyhow::Error> {
        task::spawn({
            let gossip_node = self.inner.clone();
            let socket = socket.clone();
            let address = self.address.clone();

            debug!("Starting transmitter...");

            async move {
                task::sleep(Duration::from_secs(1)).await;
                loop {
                    let to_nodes: Vec<SocketAddr> = gossip_node
                        .read()
                        .await
                        .get_random_nodes(&[address])
                        .iter()
                        .map(|a| a.clone())
                        .collect();

                    send_msg_to_nodes(socket.clone(), &to_nodes, Message::new(Action::Ping))
                        .await?;

                    task::sleep(Duration::from_secs(5)).await;

                    for to_node in &to_nodes {
                        if gossip_node.read().await.members.contains_key(&to_node) {
                            let timestamp = gossip_node.read().await.members.get(&to_node).cloned();
                            let timestamp_difference =
                                utils::time_difference_now(&timestamp.unwrap());
                            if timestamp_difference > 5 && timestamp_difference < 10 {
                                gossip_node
                                    .write()
                                    .await
                                    .suspected_nodes
                                    .set(*to_node, address);

                                // Tell other nodes to Ping
                                send_msg_to_nodes(
                                    socket.clone(),
                                    &gossip_node.read().await.get_random_nodes(&[*to_node]),
                                    Message::new(Action::PingAddress(to_node.clone())),
                                )
                                .await?;
                            }
                        }
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn membership_syncer(
        &self,
        socket: Arc<RwLock<UdpSocket>>,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let address = self.address.clone();
            let gossip_node = self.inner.clone();

            debug!("Starting membership syncer...");

            async move {
                task::sleep(Duration::from_secs(1)).await;

                loop {
                    send_msg_to_nodes(
                        socket.clone(),
                        &gossip_node.read().await.get_random_nodes(&[address]),
                        Message::new(Action::RequestMembershipList),
                    )
                    .await?;

                    task::sleep(Duration::from_secs(5)).await;

                    debug!("{:?}", gossip_node.read().await.members);
                }

                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn processor(
        &self,
        socket: Arc<RwLock<UdpSocket>>,
        rx: Receiver<Response>,
        event_sender: Sender<Event>,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let gossip_node = self.inner.clone();
            let socket = socket.clone();
            let address = self.address.clone();

            debug!("Starting processor...");

            async move {
                loop {
                    let response =
                        async_std::future::timeout(Duration::from_millis(5), rx.recv()).await;
                    match response {
                        Ok(response) => match response {
                            Ok(Response {
                                address: from_address,
                                message,
                            }) => {
                                if !gossip_node.read().await.members.contains_key(&from_address) {
                                    event_sender
                                        .send(Event::Joined(from_address, message.timestamp))
                                        .await?;
                                }

                                gossip_node
                                    .write()
                                    .await
                                    .members
                                    .touch(&from_address, message.timestamp);

                                match message.action {
                                    Action::Ping => {
                                        send_msg(
                                            socket.clone(),
                                            from_address,
                                            Message::new(Action::Pong),
                                        )
                                        .await?;
                                    }
                                    Action::Pong => {
                                        if gossip_node
                                            .read()
                                            .await
                                            .suspected_nodes
                                            .contains_key(&from_address)
                                        {
                                            let suspected_by_nodes = gossip_node
                                                .read()
                                                .await
                                                .suspected_nodes
                                                .get(&from_address)
                                                .cloned();

                                            for node in suspected_by_nodes.unwrap().iter() {
                                                if node != &address {
                                                    send_msg(
                                                        socket.clone(),
                                                        *node,
                                                        Message::new(Action::PongFrom(
                                                            from_address,
                                                            message.timestamp,
                                                        )),
                                                    )
                                                    .await?;
                                                }
                                            }
                                        }
                                        gossip_node
                                            .write()
                                            .await
                                            .suspected_nodes
                                            .remove(from_address);
                                    }
                                    Action::PingAddress(suspected_node_address) => {
                                        debug!(
                                            "{:?} wants to PingAddress({:?})",
                                            from_address, suspected_node_address
                                        );

                                        gossip_node
                                            .write()
                                            .await
                                            .suspected_nodes
                                            .set(suspected_node_address, from_address);
                                        send_msg(
                                            socket.clone(),
                                            suspected_node_address,
                                            Message::new(Action::Ping),
                                        )
                                        .await?;
                                    }
                                    Action::DeclaredDead(dec_dead_node_addr) => {
                                        debug!("{:?} is DeclaredDead", dec_dead_node_addr);

                                        if dec_dead_node_addr == address {
                                            // I'm not dead
                                            send_msg(
                                                socket.clone(),
                                                from_address,
                                                Message::new(Action::NotDead(address, Utc::now())),
                                            )
                                            .await?;
                                        } else {
                                            gossip_node
                                                .write()
                                                .await
                                                .suspected_nodes
                                                .set(dec_dead_node_addr, from_address);
                                        }
                                    }
                                    Action::NotDead(
                                        not_dead_node_addr,
                                        not_dead_node_timestamp,
                                    ) => {
                                        gossip_node
                                            .write()
                                            .await
                                            .members
                                            .touch(&not_dead_node_addr, not_dead_node_timestamp);
                                    }
                                    Action::RequestMembershipList => {
                                        let nodes: MembershipList = MembershipList::from(
                                            gossip_node.read().await.members.clone(),
                                        );

                                        send_msg(
                                            socket.clone(),
                                            from_address,
                                            Message::new(Action::MembershipList(nodes)),
                                        )
                                        .await?;
                                    }
                                    Action::MembershipList(membership_list) => {
                                        gossip_node
                                            .write()
                                            .await
                                            .process_membership_list(membership_list)?;
                                    }
                                    Action::Dead(dead_node_addr) => {
                                        debug!("{:?} is DEAD", dead_node_addr);
                                        gossip_node.write().await.members.remove(&dead_node_addr);
                                        gossip_node
                                            .write()
                                            .await
                                            .suspected_nodes
                                            .remove(dead_node_addr);
                                        event_sender
                                            .send(Event::Dead(dead_node_addr, message.timestamp))
                                            .await?;
                                    }
                                    Action::Leave => {
                                        debug!("{:?} wants to LEAVE", from_address);
                                        gossip_node.write().await.members.remove(&from_address);
                                        gossip_node
                                            .write()
                                            .await
                                            .suspected_nodes
                                            .remove(from_address);

                                        event_sender
                                            .send(Event::Left(from_address, message.timestamp))
                                            .await?;
                                    }
                                    Action::Join => {
                                        debug!("{:?} wants to JOIN", from_address);
                                        // This will be added by the default `touch` at the top.
                                        event_sender
                                            .send(Event::Joined(from_address, message.timestamp))
                                            .await?;
                                    }
                                    Action::Joined(joined_node_addr, joined_node_timestamp) => {
                                        debug!("{:?} JOINED", joined_node_addr);

                                        if !gossip_node
                                            .read()
                                            .await
                                            .members
                                            .contains_key(&joined_node_addr)
                                        {
                                            let to_nodes: Vec<SocketAddr> = gossip_node
                                                .read()
                                                .await
                                                .get_random_nodes(&[address])
                                                .iter()
                                                .map(|a| a.clone())
                                                .collect();

                                            for node in to_nodes {
                                                send_msg(
                                                    socket.clone(),
                                                    node.clone(),
                                                    message.clone(),
                                                )
                                                .await?;
                                            }
                                        }

                                        gossip_node
                                            .write()
                                            .await
                                            .members
                                            .insert(joined_node_addr, joined_node_timestamp);
                                    }
                                    Action::PongFrom(indirect_node_addr, node_timestamp) => {
                                        // I have requested a node to PING
                                        // another node since I was not able
                                        // to PING directly.

                                        debug!("Received PongFrom({:?})", indirect_node_addr);
                                        gossip_node
                                            .write()
                                            .await
                                            .members
                                            .touch(&indirect_node_addr, node_timestamp);

                                        if gossip_node
                                            .read()
                                            .await
                                            .suspected_nodes
                                            .contains_key(&from_address)
                                        {
                                            let suspected_by_nodes = gossip_node
                                                .read()
                                                .await
                                                .suspected_nodes
                                                .get(&from_address)
                                                .cloned();

                                            for node in suspected_by_nodes.unwrap().iter() {
                                                if node != &address {
                                                    send_msg(
                                                        socket.clone(),
                                                        *node,
                                                        Message::new(Action::PongFrom(
                                                            from_address,
                                                            message.timestamp,
                                                        )),
                                                    )
                                                    .await?;
                                                }
                                            }
                                        }
                                        gossip_node
                                            .write()
                                            .await
                                            .suspected_nodes
                                            .remove(from_address);
                                    }
                                }
                            }
                            Err(err) => error!("Malformed message: {:?}", err),
                        },
                        Err(_err) => task::sleep(Duration::from_millis(100)).await,
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn handler(&self, event_receiver: Receiver<Event>) -> Result<(), anyhow::Error> {
        async move {
            loop {
                let event = future::timeout(Duration::from_millis(5), event_receiver.recv()).await;

                match event {
                    Ok(node_event) => match node_event {
                        Ok(node_event) => match node_event {
                            Event::Dead(dead_node_addr, dead_node_inform_time) => {
                                println!("Dead {:?}, {:?}", dead_node_addr, dead_node_inform_time);
                            }
                            Event::Joined(node_addr, node_timestamp) => {
                                println!("Joined {:?}, {:?}", node_addr, node_timestamp);
                            }
                            _ => println!("Other events"),
                        },
                        Err(err) => eprintln!("Error: {:?}", err),
                    },
                    _ => task::sleep(Duration::from_secs(1)).await,
                };
            }
            Ok::<(), anyhow::Error>(())
        }
        .await?;
        Ok(())
    }
}
