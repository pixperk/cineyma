use std::{future::Future, io, pin::Pin, sync::Arc};

use tokio::net::TcpListener;

use crate::remote::{proto::Envelope, Connection, TcpConnection};

/// Async handler for incoming envelopes
pub type EnvelopeHandler = Arc<
    dyn Fn(Envelope) -> Pin<Box<dyn Future<Output = Option<Envelope>> + Send>> + Send + Sync
>;

///remote server accepts connections and dispatches to local actors
pub struct RemoteServer {
    listener: TcpListener,
    handler: EnvelopeHandler,
}

impl RemoteServer {
    pub async fn bind(addr: &str, handler: EnvelopeHandler) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener, handler })
    }

    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    ///run the server to accept connections
    pub async fn run(self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, peer)) => {
                    println!("Accepted connection from {:?}", peer);
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        let mut conn = TcpConnection::new(stream);

                        loop {
                            match conn.recv().await {
                                Ok(envelope) => {
                                    println!("Received: target={}", envelope.target_actor);

                                    //call handler to process (async)
                                    if let Some(response) = (handler)(envelope).await {
                                        if let Err(e) = conn.send(response).await {
                                            eprintln!("Failed to send response: {:?}", e);
                                            break;
                                        }
                                    }
                                }

                                Err(_) => break, //conn closed
                            }
                        }
                    });
                }
                Err(e) => eprintln!("Accept error: {:?}", e),
            }
        }
    }
}
