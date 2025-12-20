use crate::remote::{
    proto::{Envelope, GossipMessage, NodeInfo},
    Connection, TcpConnection, TcpTransport, Transport, TransportError,
};
use std::{collections::HashMap, sync::Arc};

use bytes::BytesMut;
use prost::Message;
use tokio::{net::TcpListener, sync::RwLock};

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

    ///start gossip server (listen for incoming gossip messages)
    pub async fn start_gossip_server(self: Arc<Self>, gossip_port: u16) -> std::io::Result<()> {
        let addr = format!("0.0.0.0:{}", gossip_port);
        let listener = TcpListener::bind(&addr).await?;

        loop {
            let (stream, peer) = listener.accept().await?;
            let cluster = self.clone();

            tokio::spawn(async move {
                let mut conn = TcpConnection::new(stream);

                //receive gossip message
                if let Ok(envelope) = conn.recv().await {
                    if let Ok(their_gossip) = GossipMessage::decode(envelope.payload.as_slice()) {
                        println!("[{}] Received gossip from {}", cluster.local_node.id, peer);

                        //merge gossip
                        cluster.merge_gossip(their_gossip).await;

                        //send our gossip back
                        let our_gossip = cluster.create_gossip_message().await;
                        let mut buf = BytesMut::new();

                        if let Err(e) = our_gossip.encode(&mut buf) {
                            eprintln!(
                                "[{}] Failed to encode gossip message: {}",
                                cluster.local_node.id, e
                            );
                            return;
                        }

                        let resp = Envelope {
                            message_type: "gossip".to_string(),
                            payload: buf.to_vec(),
                            correlation_id: 0,
                            sender_node: cluster.local_node.id.clone(),
                            target_actor: "".to_string(),
                            is_response: true,
                        };

                        let _ = conn.send(resp).await;
                    }
                }
            });
        }
    }

    pub async fn send_gossip_to(&self, peer: &Node) -> Result<(), TransportError> {
        let our_gossip = self.create_gossip_message().await;
        let mut buf = BytesMut::new();

        if let Err(e) = our_gossip.encode(&mut buf) {
            eprintln!("[{}] Failed to encode gossip: {}", self.local_node.id, e);
            return Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )));
        }

        //wrap in envelope
        let envelope = Envelope {
            message_type: "gossip".to_string(),
            payload: buf.to_vec(),
            correlation_id: 0,
            sender_node: self.local_node.id.clone(),
            target_actor: "".to_string(),
            is_response: false,
        };

        //connect to peer
        let transport = TcpTransport;
        let mut conn = transport.connect(&peer.addr).await?;

        //send gossip
        conn.send(envelope).await?;

        //receive their gossip
        if let Ok(response) = conn.recv().await {
            if let Ok(their_gossip) = GossipMessage::decode(response.payload.as_slice()) {
                println!(
                    "[{}] Received gossip response from {}",
                    self.local_node.id, peer.id
                );
                self.merge_gossip(their_gossip).await;
            }
        }

        Ok(())
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
