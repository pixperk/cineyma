use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{
    sync::{mpsc, oneshot, Mutex},
    time::timeout,
};

use crate::remote::{proto::Envelope, Connection, TcpConnection, TransportError};

///a pending request waiting for a response
type PendingRequest = oneshot::Sender<Result<Envelope, TransportError>>;

enum ClientCommand {
    Send {
        envelope: Envelope,
        response_tx: Option<PendingRequest>,
    },
    Close,
}

///remote client for sending messages to remote actors
#[derive(Clone)]
pub struct RemoteClient {
    cmd_tx: mpsc::Sender<ClientCommand>,
}

impl RemoteClient {
    pub fn new(mut conn: TcpConnection) -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientCommand>(32);
        let pending_requests: Arc<Mutex<HashMap<u64, PendingRequest>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending_requests.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            ClientCommand::Send {envelope, response_tx} => {
                                let correlation_id = envelope.correlation_id;

                                //track pending request if response is expected
                                if let Some(tx) = response_tx {
                                    let mut pending = pending_clone.lock().await;
                                    pending.insert(correlation_id, tx);
                                }

                                //send the envelope
                                if let Err(e) = conn.send(envelope).await {
                                    if let Some(tx) = pending_clone.lock().await.remove(&correlation_id) {
                                    let _ = tx.send(Err(e));
                                    }
                                }
                            }

                            ClientCommand::Close => {
                                break;
                            }
                        }
                    }
                    //incoming message
                    result = conn.recv() => {
                        match result {
                            Ok(envelope) => {
                                if envelope.is_response {
                                    if let Some(tx) = pending_clone.lock().await.remove(&envelope.correlation_id) {
                                        let _ = tx.send(Ok(envelope));
                                    }
                                }
                            }
                            Err(TransportError::Disconnected) => break,
                            Err(_) => continue,
                        }
                    }
                }
            }
        });

        Self { cmd_tx }
    }

    /// Fire-and-forget send
    pub async fn do_send(&self, envelope: Envelope) -> Result<(), TransportError> {
        self.cmd_tx
            .send(ClientCommand::Send {
                envelope,
                response_tx: None,
            })
            .await
            .map_err(|_| TransportError::Disconnected)
    }

    /// Send and wait for response
    pub async fn send(&self, envelope: Envelope) -> Result<Envelope, TransportError> {
        let (tx, rx) = oneshot::channel();

        self.cmd_tx
            .send(ClientCommand::Send {
                envelope,
                response_tx: Some(tx),
            })
            .await
            .map_err(|_| TransportError::Disconnected)?;

        rx.await.map_err(|_| TransportError::Disconnected)?
    }

    pub async fn send_timeout(
        &self,
        envelope: Envelope,
        duration: Duration,
    ) -> Result<Envelope, TransportError> {
        match timeout(duration, self.send(envelope)).await {
            Ok(result) => result,
            Err(_) => Err(TransportError::Timeout),
        }
    }
}
