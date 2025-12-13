use tokio::sync::{mpsc, oneshot};

use crate::{
    envelope::{Envelope, MessageEnvelope},
    error::MailboxError,
    Actor, Handler, Message,
};

pub struct Addr<A: Actor> {
    sender: mpsc::UnboundedSender<Box<dyn Envelope<A>>>,
}

impl<A: Actor> Addr<A> {
    pub fn new(sender: mpsc::UnboundedSender<Box<dyn Envelope<A>>>) -> Self {
        Self { sender }
    }

    ///Send message and wait for response
    pub async fn send<M>(&self, msg: M) -> Result<M::Result, MailboxError>
    where
        A: Handler<M>,
        M: Message,
    {
        let (tx, rx) = oneshot::channel();
        let envelope = MessageEnvelope::with_response(msg, tx);
        self.sender
            .send(Box::new(envelope))
            .map_err(|_| MailboxError::MailboxClosed)?;

        rx.await.map_err(|_| MailboxError::MailboxClosed)
    }

    pub async fn send_timeout<M>(
        &self,
        msg: M,
        timeout: std::time::Duration,
    ) -> Result<M::Result, MailboxError>
    where
        A: Handler<M>,
        M: Message,
    {
        let (tx, rx) = oneshot::channel();
        let envelope = MessageEnvelope::with_response(msg, tx);
        self.sender
            .send(Box::new(envelope))
            .map_err(|_| MailboxError::MailboxClosed)?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(res) => res.map_err(|_| MailboxError::MailboxClosed),
            Err(_) => Err(MailboxError::Timeout),
        }
    }

    ///Fire and forget message sending
    pub fn do_send<M>(&self, msg: M)
    where
        A: Handler<M>,
        M: Message,
    {
        let envelope = MessageEnvelope::new(msg);
        let _ = self.sender.send(Box::new(envelope));
    }
}

impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}
