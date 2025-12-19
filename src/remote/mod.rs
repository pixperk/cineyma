mod addr;
mod client;
mod handler;
mod registry;
mod server;
mod tcp;
mod transport;

pub use addr::{NodeId, RemoteActorId, RemoteAddr};
pub use client::RemoteClient;
pub use handler::{make_handler, make_tell_handler, LocalNode, MessageRouter};
pub use registry::{deserialize_payload, register_message};
pub use server::{EnvelopeHandler, RemoteServer};
pub use tcp::{EnvelopeCodec, TcpConnection, TcpTransport};
pub use transport::{Connection, Transport, TransportError};

use bytes::{Bytes, BytesMut};
use prost::Message as ProstMessage;

use crate::{remote::proto::Envelope, Message};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/cinema.rs"));
}

/// Trait for remote messages (can be sent over the network).
/// To be remotable, a message must implement this trait.
/// The type_id is auto-derived from Rust's type name.
pub trait RemoteMessage: Message + ProstMessage + Default + Clone {
    fn type_id() -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl Envelope {
    ///create an envelope from a remote message
    pub fn from_message<M: RemoteMessage>(
        msg: &M,
        correlation_id: u64,
        sender_node: &str,
        target_actor: &str,
    ) -> Self {
        let mut payload = BytesMut::new();
        msg.encode(&mut payload).expect("encode failed");

        Envelope {
            message_type: M::type_id().to_string(),
            payload: payload.to_vec(),
            correlation_id,
            sender_node: sender_node.to_string(),
            target_actor: target_actor.to_string(),
            is_response: false,
        }
    }

    ///serialize the envelope to bytes
    pub fn to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::new();
        self.encode(&mut buf).expect("encode failed");
        buf.freeze()
    }

    /// Decode envelope from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, prost::DecodeError> {
        Envelope::decode(data)
    }
}
