use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::remote::{proto::Envelope, RemoteClient, RemoteMessage, TransportError};

///global correlation id counter
static CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_correlation_id() -> u64 {
    CORRELATION_ID.fetch_add(1, Ordering::Relaxed)
}

///unique identifier for a remote node
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

///address of a remote actor
#[derive(Debug, Clone)]
pub struct RemoteActorId {
    pub node: NodeId,
    pub actor_name: String,
}

///remote address - points to an actor on another node
pub struct RemoteAddr<A> {
    pub id: RemoteActorId,
    local_node: NodeId,
    client: RemoteClient,
    _phantom: PhantomData<A>,
}

impl<A> RemoteAddr<A> {
    pub fn new(
        local_node_id: &str,
        remote_node_id: &str,
        actor_name: &str,
        client: RemoteClient,
    ) -> Self {
        Self {
            id: RemoteActorId {
                node: NodeId(remote_node_id.to_string()),
                actor_name: actor_name.to_string(),
            },
            local_node: NodeId(local_node_id.to_string()),
            client,
            _phantom: PhantomData,
        }
    }

    ///fire and forget send to remote actor
    pub async fn do_send<M>(&self, msg: M) -> Result<(), TransportError>
    where
        M: RemoteMessage,
    {
        let envelope = Envelope::from_message(
            &msg,
            next_correlation_id(),
            &self.local_node.0,
            &self.id.actor_name,
        );

        self.client.do_send(envelope).await
    }

    pub async fn send<M>(&self, msg: M) -> Result<Envelope, TransportError>
    where
        M: RemoteMessage,
    {
        let envelope = Envelope::from_message(
            &msg,
            next_correlation_id(),
            &self.local_node.0,
            &self.id.actor_name,
        );
        self.client.send(envelope).await
    }
}
