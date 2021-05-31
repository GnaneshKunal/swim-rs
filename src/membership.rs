use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use std::{
    collections::HashMap,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

type MembershipListType = HashMap<SocketAddr, DateTime<Utc>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MembershipList(MembershipListType);

impl MembershipList {
    pub fn new(addresses: &[SocketAddr]) -> Self {
        let mut h = HashMap::new();
        for address in addresses {
            h.insert(*address, Utc::now());
        }
        Self(h)
    }

    pub fn new_hm(nodes: MembershipListType) -> Self {
        Self(nodes)
    }

    // pub async fn touch<F, Fut>(&mut self, address: &SocketAddr, time: DateTime<Utc>, cb: F)
    // where
    //     F: Fn() -> Fut,
    //     Fut: Future<Output = ()>,
    pub fn touch(&mut self, address: &SocketAddr, time: DateTime<Utc>) {
        self.insert(*address, time);
        // cb().await;
    }
}

impl From<&[SocketAddr]> for MembershipList {
    fn from(seed_nodes: &[SocketAddr]) -> Self {
        Self::new(seed_nodes)
    }
}

impl From<MembershipListType> for MembershipList {
    fn from(seed_nodes: MembershipListType) -> Self {
        Self::new_hm(seed_nodes)
    }
}

impl Deref for MembershipList {
    type Target = MembershipListType;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MembershipList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
