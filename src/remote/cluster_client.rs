use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use crate::remote::{
    cluster::ClusterNode,
    proto::{cluster_message, ClusterMessage, Envelope},
    RemoteClient, TcpTransport, Transport, TransportError,
};
use bytes::BytesMut;
use prost::Message;

///manages persistent connections to cluster nodes using remoteclient
struct ConnectionPool {
    ///node_addr -> remoteclient
    clients: HashMap<String, RemoteClient>,
}

impl ConnectionPool {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    ///get or create a remoteclient for a node
    async fn get_or_connect(
        &mut self,
        node_addr: &str,
        transport: &TcpTransport,
    ) -> Result<RemoteClient, TransportError> {
        if !self.clients.contains_key(node_addr) {
            let tcp_conn = transport.connect(node_addr).await?;
            let client = RemoteClient::new(tcp_conn);
            self.clients.insert(node_addr.to_string(), client);
        }
        Ok(self.clients.get(node_addr).unwrap().clone())
    }

    fn remove(&mut self, node_addr: &str) {
        self.clients.remove(node_addr);
    }
}

///client for sending messages to actors in the cluster
///uses cluster registry for discovery and remoteclient for correlation tracking
pub struct ClusterClient {
    cluster: Arc<ClusterNode>,
    pool: Arc<Mutex<ConnectionPool>>,
    transport: TcpTransport,
    local_node_id: String,
}

impl ClusterClient {
    pub fn new(cluster: Arc<ClusterNode>) -> Self {
        let local_node_id = cluster.local_node.id.clone();
        Self {
            cluster,
            pool: Arc::new(Mutex::new(ConnectionPool::new())),
            transport: TcpTransport,
            local_node_id,
        }
    }

    ///send request-response message to actor
    ///1. lookup actor location in cluster registry
    ///2. get or create connection to target node
    ///3. wrap message in clustermessage envelope
    ///4. send and wait for response with correlation tracking
    pub async fn send_to_actor(
        &self,
        actor_id: &str,
        envelope: Envelope,
    ) -> Result<Envelope, TransportError> {
        //lookup actor in cluster
        let (node_id, _actor_type) =
            self.cluster.lookup_actor(actor_id).await.ok_or_else(|| {
                TransportError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("actor {} not found in cluster", actor_id),
                ))
            })?;

        //get node address
        let members = self.cluster.get_members().await;
        let node = members.iter().find(|n| n.id == node_id).ok_or_else(|| {
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("node {} not found", node_id),
            ))
        })?;

        //wrap actor envelope in clustermessage
        let cluster_msg = ClusterMessage {
            payload: Some(cluster_message::Payload::Envelope(envelope)),
        };

        let mut buf = BytesMut::new();
        cluster_msg.encode(&mut buf).map_err(|e| {
            TransportError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        //wrap in transport envelope
        let transport_envelope = Envelope {
            message_type: "cluster".to_string(),
            payload: buf.to_vec(),
            correlation_id: 0,
            sender_node: self.local_node_id.clone(),
            target_actor: "".to_string(),
            is_response: false,
        };

        //get or create connection - remoteclient handles correlation tracking
        let mut pool = self.pool.lock().await;
        let client = pool
            .get_or_connect(&node.addr, &self.transport)
            .await
            .map_err(|e| {
                //on connection failure, remove from pool to force reconnect next time
                pool.remove(&node.addr);
                e
            })?;
        drop(pool); //release lock before async send

        //send via remoteclient (handles correlation id tracking internally)
        let response = client.send(transport_envelope).await.map_err(|e| {
            //on send/recv failure, clear connection from pool
            let pool = self.pool.clone();
            let node_addr = node.addr.clone();
            tokio::spawn(async move {
                pool.lock().await.remove(&node_addr);
            });
            e
        })?;

        //unwrap clustermessage
        if let Ok(cluster_resp) = ClusterMessage::decode(response.payload.as_slice()) {
            if let Some(cluster_message::Payload::Envelope(actor_response)) = cluster_resp.payload {
                return Ok(actor_response);
            }
        }

        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid response format",
        )))
    }

    ///send fire-and-forget message to actor (no response expected)
    pub async fn do_send_to_actor(
        &self,
        actor_id: &str,
        envelope: Envelope,
    ) -> Result<(), TransportError> {
        //lookup actor in cluster
        let (node_id, _actor_type) =
            self.cluster.lookup_actor(actor_id).await.ok_or_else(|| {
                TransportError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("actor {} not found in cluster", actor_id),
                ))
            })?;

        //get node address
        let members = self.cluster.get_members().await;
        let node = members.iter().find(|n| n.id == node_id).ok_or_else(|| {
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("node {} not found", node_id),
            ))
        })?;

        //wrap actor envelope in clustermessage
        let cluster_msg = ClusterMessage {
            payload: Some(cluster_message::Payload::Envelope(envelope)),
        };

        let mut buf = BytesMut::new();
        cluster_msg.encode(&mut buf).map_err(|e| {
            TransportError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        //wrap in transport envelope
        let transport_envelope = Envelope {
            message_type: "cluster".to_string(),
            payload: buf.to_vec(),
            correlation_id: 0,
            sender_node: self.local_node_id.clone(),
            target_actor: "".to_string(),
            is_response: false,
        };

        //get or create connection
        let mut pool = self.pool.lock().await;
        let client = pool
            .get_or_connect(&node.addr, &self.transport)
            .await
            .map_err(|e| {
                pool.remove(&node.addr);
                e
            })?;
        drop(pool);

        //fire-and-forget send
        client.do_send(transport_envelope).await
    }

    ///create a remote address for an actor (doesn't lookup yet, lookup happens on send)
    pub fn remote_addr<A>(&self, actor_id: &str) -> ClusterRemoteAddr<A> {
        ClusterRemoteAddr {
            actor_id: actor_id.to_string(),
            client: self.clone(),
            _phantom: std::marker::PhantomData,
        }
    }

    ///clear connection to a specific node (useful after network errors)
    pub async fn clear_connection(&self, node_addr: &str) {
        self.pool.lock().await.remove(node_addr);
    }
}

impl Clone for ClusterClient {
    fn clone(&self) -> Self {
        Self {
            cluster: self.cluster.clone(),
            pool: self.pool.clone(),
            transport: TcpTransport,
            local_node_id: self.local_node_id.clone(),
        }
    }
}

///remote address that uses cluster discovery
///acts as a typed handle to an actor somewhere in the cluster
pub struct ClusterRemoteAddr<A> {
    actor_id: String,
    client: ClusterClient,
    _phantom: std::marker::PhantomData<A>,
}

impl<A> ClusterRemoteAddr<A> {
    ///send request-response message and return raw envelope
    pub async fn send<M>(&self, msg: M) -> Result<Envelope, TransportError>
    where
        M: crate::remote::RemoteMessage,
    {
        use crate::remote::addr::next_correlation_id;

        let envelope = Envelope::from_message(
            &msg,
            next_correlation_id(),
            &self.client.local_node_id,
            &self.actor_id,
        );

        self.client.send_to_actor(&self.actor_id, envelope).await
    }

    ///send request-response message and decode typed response
    ///this is the high-level api that automatically decodes the response
    pub async fn call<M>(&self, msg: M) -> Result<M::Result, TransportError>
    where
        M: crate::remote::RemoteMessage,
        M::Result: crate::remote::RemoteMessage,
    {
        use prost::Message as ProstMessage;

        let response_envelope = self.send(msg).await?;

        //decode the response
        M::Result::decode(response_envelope.payload.as_slice()).map_err(|e| {
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to decode response: {}", e),
            ))
        })
    }

    ///send fire-and-forget message to actor
    pub async fn do_send<M>(&self, msg: M) -> Result<(), TransportError>
    where
        M: crate::remote::RemoteMessage,
    {
        use crate::remote::addr::next_correlation_id;

        let envelope = Envelope::from_message(
            &msg,
            next_correlation_id(),
            &self.client.local_node_id,
            &self.actor_id,
        );

        self.client.do_send_to_actor(&self.actor_id, envelope).await
    }
}
