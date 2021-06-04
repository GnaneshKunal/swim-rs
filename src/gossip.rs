use async_std::{
    channel::{unbounded, Receiver, Sender},
    future,
    net::UdpSocket,
    sync::{Arc, RwLock},
    task,
};
use chrono::Utc;
use log::debug;
use rand::{seq::IteratorRandom, thread_rng};
use std::{
    fmt::Debug,
    marker::{Send, Sync},
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
    socket: Arc<RwLock<UdpSocket>>,
    inner: Arc<RwLock<GossipInner>>,
}

impl Gossip {
    pub async fn new<A: ToSocketAddrs>(address: A, seed_nodes: &[A]) -> Self {
        let address = address.to_socket_addrs().unwrap().next().unwrap();
        let socket: Arc<RwLock<UdpSocket>> = Arc::new(RwLock::new(
            UdpSocket::bind(address).await.expect("It should work"),
        ));

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
            socket,
            inner: Arc::new(RwLock::new(gossip_inner)),
        }
    }

    async fn get_address(&self) -> SocketAddr {
        self.socket.read().await.local_addr().unwrap()
    }

    pub async fn get_random_nodes(&self) -> Vec<SocketAddr> {
        self.inner
            .read()
            .await
            .get_random_nodes(&[self.get_address().await])
    }

    pub async fn start<T: 'static + Send + Sync + Serialize + Clone + Debug + DeserializeOwned>(
        &self,
        event_sender: Sender<Event<T>>,
    ) -> Result<(), anyhow::Error> {
        let (sx, rx) = unbounded();

        let gossip_node = &self;

        // `try_join` is better than `join`
        futures::try_join!(
            gossip_node.transmitter::<T>(),
            gossip_node.receiver::<T>(sx),
            gossip_node.processor::<T>(rx, event_sender),
            gossip_node.checker::<T>(),
            gossip_node.membership_syncer::<T>(),
        )?;

        Ok(())
    }

    pub async fn receiver<
        'a,
        T: 'static + Send + Sync + Serialize + Clone + Debug + DeserializeOwned,
    >(
        &self,
        sx: Sender<Response<T>>,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let socket = self.socket.clone();

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
                                let msg: Message<T> = bytes_to_msg(&buf);

                                // send to inbox
                                sx.send(Response::new(peer, msg)).await?;
                            }
                            _ => (),
                        },
                        _ => (),
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn checker<
        'a,
        T: 'static + Send + Sync + Serialize + Debug + Clone + Deserialize<'a>,
    >(
        &self,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let socket = self.socket.clone();
            let gossip_node = self.inner.clone();
            // let address = self.address.clone();
            let address = self.get_address().await;

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
                                Message::new(Action::<T>::Dead(node)),
                            )
                            .await?;
                        } else if node_timestamp_difference > 20 {
                            send_msg_to_nodes(
                                socket.clone(),
                                &gossip_node.read().await.get_random_nodes(&[node]),
                                Message::new(Action::<T>::DeclaredDead(node)),
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

    pub async fn transmitter<
        'a,
        T: 'static + Send + Sync + Debug + Serialize + Clone + Deserialize<'a>,
    >(
        &self,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let gossip_node = self.inner.clone();
            let socket = self.socket.clone();
            let address = self.get_address().await;

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

                    send_msg_to_nodes(socket.clone(), &to_nodes, Message::new(Action::<T>::Ping))
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
                                    Message::new(Action::<T>::PingAddress(to_node.clone())),
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

    pub async fn membership_syncer<
        'a,
        T: 'static + Send + Sync + Serialize + Debug + Clone + Deserialize<'a>,
    >(
        &self,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let address = self.get_address().await;
            let gossip_node = self.inner.clone();
            let socket = self.socket.clone();

            debug!("Starting membership syncer...");

            async move {
                task::sleep(Duration::from_secs(1)).await;

                loop {
                    send_msg_to_nodes(
                        socket.clone(),
                        &gossip_node.read().await.get_random_nodes(&[address]),
                        Message::new(Action::<T>::RequestMembershipList),
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

    pub async fn processor<
        'a,
        T: 'static + Send + Sync + Serialize + Clone + Debug + Deserialize<'a>,
    >(
        &self,
        rx: Receiver<Response<T>>,
        event_sender: Sender<Event<T>>,
    ) -> Result<(), anyhow::Error> {
        task::spawn({
            let gossip_node = self.inner.clone();
            let socket = self.socket.clone();
            let address = self.get_address().await;

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
                                    Action::Data(x) => println!("Data: {:?}", x),
                                    Action::Ping => {
                                        send_msg(
                                            socket.clone(),
                                            from_address,
                                            Message::new(Action::<T>::Pong),
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
                                                        Message::new(Action::<T>::PongFrom(
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
                                            Message::new(Action::<T>::Ping),
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
                                                Message::new(Action::<T>::NotDead(
                                                    address,
                                                    Utc::now(),
                                                )),
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
                                            Message::new(Action::<T>::MembershipList(nodes)),
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
                                                        Message::new(Action::<T>::PongFrom(
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
                            Err(_err) => (),
                        },
                        Err(_err) => (),
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .await?;
        Ok(())
    }

    pub async fn gossip<
        'a,
        T: 'static + Send + Sync + Serialize + Clone + Debug + Deserialize<'a>,
    >(
        &self,
        data: T,
    ) -> Result<(), anyhow::Error> {
        send_msg_to_nodes(
            self.socket.clone(),
            &self
                .inner
                .read()
                .await
                .get_random_nodes(&[self.get_address().await]),
            Message::new(Action::<T>::Data(data)),
        )
        .await?;

        Ok::<(), anyhow::Error>(())
    }

    pub async fn handler<
        'a,
        T: 'static + Send + Sync + Serialize + Clone + Debug + Deserialize<'a>,
    >(
        &self,
        event_receiver: Receiver<Event<T>>,
    ) -> Result<(), anyhow::Error> {
        async move {
            loop {
                let event = future::timeout(Duration::from_millis(5), event_receiver.recv()).await;

                match event {
                    Ok(event) => match event {
                        Ok(event) => match event {
                            Event::Dead(dead_node_addr, dead_node_inform_time) => {
                                println!("Dead {:?}, {:?}", dead_node_addr, dead_node_inform_time);
                            }
                            Event::Joined(node_addr, node_timestamp) => {
                                println!("Joined {:?}, {:?}", node_addr, node_timestamp);
                            }
                            Event::Data(data) => {
                                println!("Data: {:?}", data);
                            }
                            _ => println!("Other events"),
                        },
                        Err(err) => eprintln!("Error: {:?}", err),
                    },
                    Err(_err) => (),
                }
            }
            Ok::<(), anyhow::Error>(())
        }
        .await?;
        Ok(())
    }
}
