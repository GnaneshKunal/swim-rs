use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub struct SuspectedNodeList(pub HashMap<SocketAddr, HashSet<SocketAddr>>);

impl SuspectedNodeList {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn set(&mut self, suspected_node: SocketAddr, suspected_by: SocketAddr) {
        let node = self.0.get_mut(&suspected_node);

        if node.is_some() {
            node.unwrap().insert(suspected_by);
        } else {
            let mut nodes = HashSet::new();
            nodes.insert(suspected_by);
            self.0.insert(suspected_node, nodes);
        }
    }

    pub fn remove(&mut self, suspected_node: SocketAddr) {
        self.0.remove(&suspected_node);
    }
}

impl Deref for SuspectedNodeList {
    type Target = HashMap<SocketAddr, HashSet<SocketAddr>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SuspectedNodeList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
