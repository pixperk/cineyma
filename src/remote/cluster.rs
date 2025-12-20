use crate::remote::proto::{GossipMessage, NodeInfo};
use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

#[derive(Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub addr: String, //for tcp host:port
    pub status: NodeStatus,
}

#[derive(Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Up,
    Suspect,
    Down,
}

/// Represents a node in the cluster along with its members.
pub struct ClusterNode {
    ///our own node information
    pub local_node: Node,
    ///cluster members(node id -> Node)
    members: Arc<RwLock<HashMap<String, Node>>>,
}

impl ClusterNode {
    pub fn new(id: String, addr: String) -> Self {
        let local_node = Node {
            id: id.clone(),
            addr,
            status: NodeStatus::Up,
        };

        let mut members = HashMap::new();
        members.insert(id, local_node.clone());

        Self {
            local_node,
            members: Arc::new(RwLock::new(members)),
        }
    }

    ///add or update a member in the cluster
    pub async fn add_member(&self, node: Node) {
        let mut members = self.members.write().await;
        members.insert(node.id.clone(), node);
    }

    ///get all members in the cluster
    pub async fn get_members(&self) -> Vec<Node> {
        let members = self.members.read().await;
        members.values().cloned().collect()
    }

    ///create a gossip message with current cluster members
    pub async fn create_gossip_message(&self) -> GossipMessage {
        let members = self.members.read().await;
        let node_infos = members.values().map(|n| NodeInfo::from(n)).collect();

        GossipMessage {
            members: node_infos,
        }
    }

    pub async fn merge_gossip(&self, gossip: GossipMessage) {
        let mut members = self.members.write().await;

        for node_info in gossip.members {
            let node: Node = node_info.into();

            //update if dont know this node or status changed
            members
                .entry(node.id.clone())
                .and_modify(|existing_node| {
                    if existing_node.status != node.status {
                        *existing_node = node.clone();
                    }
                })
                .or_insert(node);
        }
    }
}

impl From<&Node> for NodeInfo {
    fn from(node: &Node) -> Self {
        NodeInfo {
            id: node.id.clone(),
            addr: node.addr.clone(),
            status: match node.status {
                NodeStatus::Up => 0,
                NodeStatus::Suspect => 1,
                NodeStatus::Down => 2,
            },
        }
    }
}

impl From<NodeInfo> for Node {
    fn from(info: NodeInfo) -> Self {
        Node {
            id: info.id,
            addr: info.addr,
            status: match info.status {
                0 => NodeStatus::Up,
                1 => NodeStatus::Suspect,
                2 => NodeStatus::Down,
                _ => NodeStatus::Down, // default to Down for unknown
            },
        }
    }
}
