use std::{future::Future, pin::Pin};

use crate::remote::proto::Envelope;

#[derive(Debug)]

/// Errors that can occur during transport operations.
pub enum TransportError {
    Io(std::io::Error),
    Decode(prost::DecodeError),
    Disconnected,
    Timeout,
}

impl From<std::io::Error> for TransportError {
    fn from(err: std::io::Error) -> Self {
        TransportError::Io(err)
    }
}
impl From<prost::DecodeError> for TransportError {
    fn from(err: prost::DecodeError) -> Self {
        TransportError::Decode(err)
    }
}

pub trait Connection: Send + Sync {
    // Send an envelope over this connection
    fn send(
        &mut self,
        envelope: Envelope,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + '_>>;

    // Receive an envelope from this connection
    fn recv(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Envelope, TransportError>> + Send + '_>>;

    /// Close the connection gracefully
    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + '_>>;
}

/// Transport abstraction for connecting to remote nodes
pub trait Transport: Send + Sync {
    type Conn: Connection;

    /// Connect to a remote address
    fn connect(
        &self,
        addr: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Conn, TransportError>> + Send + '_>>;
}
