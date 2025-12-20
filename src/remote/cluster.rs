use crate::remote::{
    proto::{cluster_message, ActorLocation, ClusterMessage, Envelope, GossipMessage, NodeInfo},
    Connection, EnvelopeHandler, TcpConnection, TcpTransport, Transport, TransportError,
};
use std::{collections::HashMap, sync::Arc};

use bytes::BytesMut;
use prost::Message;
use rand::seq::IteratorRandom;
use tokio::{net::TcpListener, sync::RwLock, time::{Duration, Instant}};

#[derive(Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub addr: String, //for tcp host:port
    pub status: NodeStatus,
}

#[derive(Clone, PartialEq, Eq, Debug)]
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
    ///last heartbeat time for each node
    last_heartbeat: Arc<RwLock<HashMap<String, Instant>>>,
    ///actor_id -> (node_id, actor_type)
    actor_registry: Arc<RwLock<HashMap<String, (String, String)>>>,
}

impl ClusterNode {
    pub fn new(id: String, addr: String) -> Self {
        let local_node = Node {
            id: id.clone(),
            addr,
            status: NodeStatus::Up,
        };

        let mut members = HashMap::new();
        members.insert(id.clone(), local_node.clone());

        let mut heartbeats = HashMap::new();
        heartbeats.insert(id, Instant::now());

        Self {
            local_node,
            members: Arc::new(RwLock::new(members)),
            last_heartbeat: Arc::new(RwLock::new(heartbeats)),
            actor_registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    ///add or update a member in the cluster
    pub async fn add_member(&self, node: Node) {
        let mut members = self.members.write().await;
        members.insert(node.id.clone(), node.clone());

        // Record heartbeat time
        let mut heartbeats = self.last_heartbeat.write().await;
        heartbeats.insert(node.id, Instant::now());
    }

    ///get all members in the cluster
    pub async fn get_members(&self) -> Vec<Node> {
        let members = self.members.read().await;
        members.values().cloned().collect()
    }

    ///register an actor running on this node
    pub async fn register_actor(&self, actor_id: String, actor_type: String) {
        let mut registry = self.actor_registry.write().await;
        registry.insert(actor_id, (self.local_node.id.clone(), actor_type));
    }

    ///lookup which node an actor is running on
    pub async fn lookup_actor(&self, actor_id: &str) -> Option<(String, String)> {
        let registry = self.actor_registry.read().await;
        registry.get(actor_id).cloned()
    }

    /// Test helper: manually insert an actor location (for testing failure scenarios)
    #[doc(hidden)]
    pub async fn test_insert_actor(&self, actor_id: String, node_id: String, actor_type: String) {
        let mut registry = self.actor_registry.write().await;
        registry.insert(actor_id, (node_id, actor_type));
    }

    ///create a gossip message with current cluster members
    pub async fn create_gossip_message(&self) -> GossipMessage {
        let members = self.members.read().await;
        let node_infos = members.values().map(|n| NodeInfo::from(n)).collect();

        let registry = self.actor_registry.read().await;
        let actor_locations = registry
            .iter()
            .map(|(actor_id, (node_id, actor_type))| ActorLocation {
                actor_id: actor_id.clone(),
                node_id: node_id.clone(),
                actor_type: actor_type.clone(),
            })
            .collect();

        GossipMessage {
            members: node_infos,
            actors: actor_locations,
        }
    }

    pub async fn merge_gossip(&self, gossip: GossipMessage, sender_node_id: &str) {
        let mut members = self.members.write().await;
        let mut heartbeats = self.last_heartbeat.write().await;

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
                .or_insert(node.clone());
        }

        // only update heartbeat for the actual sender, not all nodes in gossip
        heartbeats.insert(sender_node_id.to_string(), Instant::now());

        // Merge actor locations
        let mut registry = self.actor_registry.write().await;
        for actor_loc in gossip.actors {
            registry.insert(
                actor_loc.actor_id,
                (actor_loc.node_id, actor_loc.actor_type),
            );
        }
    }

    ///start cluster server (handles both gossip and actor messages)
    pub async fn start_server(
        self: Arc<Self>,
        port: u16,
        actor_handler: Option<EnvelopeHandler>,
    ) -> std::io::Result<()> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await?;

        loop {
            let (stream, _peer) = listener.accept().await?;
            let cluster = self.clone();
            let handler = actor_handler.clone();

            tokio::spawn(async move {
                let mut conn = TcpConnection::new(stream);

                loop {
                    match conn.recv().await {
                        Ok(envelope) => {
                            //decode as clustermessage
                            if let Ok(cluster_msg) = ClusterMessage::decode(envelope.payload.as_slice()) {
                                match cluster_msg.payload {
                                    Some(cluster_message::Payload::Gossip(gossip)) => {
                                        cluster.merge_gossip(gossip, &envelope.sender_node).await;

                                        //send our gossip back
                                        let our_gossip = cluster.create_gossip_message().await;
                                        let mut buf = BytesMut::new();

                                        let cluster_resp = ClusterMessage {
                                            payload: Some(cluster_message::Payload::Gossip(our_gossip)),
                                        };

                                        if cluster_resp.encode(&mut buf).is_ok() {
                                            let resp = Envelope {
                                                message_type: "cluster".to_string(),
                                                payload: buf.to_vec(),
                                                correlation_id: 0,
                                                sender_node: cluster.local_node.id.clone(),
                                                target_actor: "".to_string(),
                                                is_response: true,
                                            };
                                            let _ = conn.send(resp).await;
                                        }
                                    }
                                    Some(cluster_message::Payload::Envelope(actor_envelope)) => {
                                        if let Some(ref handler) = handler {
                                            if let Some(response) = handler(actor_envelope).await {
                                                //wrap response in clustermessage
                                                let mut buf = BytesMut::new();
                                                let cluster_resp = ClusterMessage {
                                                    payload: Some(cluster_message::Payload::Envelope(response)),
                                                };

                                                if cluster_resp.encode(&mut buf).is_ok() {
                                                    let resp = Envelope {
                                                        message_type: "cluster".to_string(),
                                                        payload: buf.to_vec(),
                                                        correlation_id: 0,
                                                        sender_node: cluster.local_node.id.clone(),
                                                        target_actor: "".to_string(),
                                                        is_response: true,
                                                    };
                                                    let _ = conn.send(resp).await;
                                                }
                                            }
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    }

    ///legacy: start gossip server without actor handling
    pub async fn start_gossip_server(self: Arc<Self>, port: u16) -> std::io::Result<()> {
        self.start_server(port, None).await
    }

    pub async fn send_gossip_to(&self, peer: &Node) -> Result<(), TransportError> {
        let our_gossip = self.create_gossip_message().await;

        //wrap in clustermessage
        let cluster_msg = ClusterMessage {
            payload: Some(cluster_message::Payload::Gossip(our_gossip)),
        };

        let mut buf = BytesMut::new();
        if let Err(e) = cluster_msg.encode(&mut buf) {
            eprintln!("[{}] failed to encode cluster message: {}", self.local_node.id, e);
            return Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )));
        }

        //wrap in envelope for transport
        let envelope = Envelope {
            message_type: "cluster".to_string(),
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
            if let Ok(cluster_resp) = ClusterMessage::decode(response.payload.as_slice()) {
                if let Some(cluster_message::Payload::Gossip(their_gossip)) = cluster_resp.payload {
                    self.merge_gossip(their_gossip, &response.sender_node).await;
                }
            }
        }

        Ok(())
    }

    /// Start periodic gossip to random peers with integrated failure detection
    pub fn start_periodic_gossip(
        self: Arc<Self>,
        interval: Duration,
        suspect_timeout: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                // Check for failed nodes (failure detection)
                let now = Instant::now();
                let mut down_nodes = Vec::new();
                {
                    let mut members = self.members.write().await;
                    let heartbeats = self.last_heartbeat.read().await;

                    for (node_id, node) in members.iter_mut() {
                        if node_id == &self.local_node.id {
                            continue; // Skip self
                        }

                        if let Some(last_seen) = heartbeats.get(node_id) {
                            let elapsed = now.duration_since(*last_seen);

                            if elapsed > suspect_timeout * 2 && node.status != NodeStatus::Down {
                                println!("[{}] Marking {} as DOWN", self.local_node.id, node_id);
                                node.status = NodeStatus::Down;
                                down_nodes.push(node_id.clone());
                            } else if elapsed > suspect_timeout && node.status == NodeStatus::Up {
                                println!("[{}] Marking {} as SUSPECT", self.local_node.id, node_id);
                                node.status = NodeStatus::Suspect;
                            }
                        }
                    }
                }

                // Clean up actors from DOWN nodes
                if !down_nodes.is_empty() {
                    let mut registry = self.actor_registry.write().await;
                    for down_node_id in &down_nodes {
                        registry.retain(|actor_id, (node_id, _)| {
                            if node_id == down_node_id {
                                println!(
                                    "[{}] Removing actor {} from DOWN node {}",
                                    self.local_node.id, actor_id, down_node_id
                                );
                                false
                            } else {
                                true
                            }
                        });
                    }
                }

                // Pick random peer (excluding self)
                let peer = {
                    let members = self.members.read().await;
                    members
                        .values()
                        .filter(|n| n.id != self.local_node.id)
                        .choose(&mut rand::rng())
                        .cloned()
                };

                if let Some(peer) = peer {
                    let _ = self.send_gossip_to(&peer).await;
                }
            }
        })
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
