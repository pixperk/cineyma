use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::{
    actor::{ActorId, AsyncHandler},
    envelope::{ActorMessage, AsyncMessageEnvelope, MessageEnvelope},
    error::MailboxError,
    message::Terminated,
    watcher::Watcher,
    Actor, Handler, Message,
};

pub struct Addr<A: Actor> {
    sender: mpsc::UnboundedSender<ActorMessage<A>>,
    id: ActorId,
    watchers: Arc<Mutex<Vec<Arc<dyn Watcher>>>>,
}

impl<A: Actor> Addr<A> {
    pub fn new(sender: mpsc::UnboundedSender<ActorMessage<A>>, id: ActorId) -> Self {
        Self {
            sender,
            id,
            watchers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn id(&self) -> ActorId {
        self.id
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
            .send(ActorMessage::Sync(Box::new(envelope)))
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
            .send(ActorMessage::Sync(Box::new(envelope)))
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
        let _ = self.sender.send(ActorMessage::Sync(Box::new(envelope)));
    }

    /// Fire and forget for async handlers
    pub fn do_send_async<M>(&self, msg: M)
    where
        A: AsyncHandler<M>,
        M: Message,
    {
        let envelope = AsyncMessageEnvelope::new(msg);
        let _ = self.sender.send(ActorMessage::Async(Box::new(envelope)));
    }

    /// Send and wait for response from async handler
    pub async fn send_async<M>(&self, msg: M) -> Result<M::Result, MailboxError>
    where
        A: AsyncHandler<M>,
        M: Message,
    {
        let (tx, rx) = oneshot::channel();
        let envelope = AsyncMessageEnvelope::with_response(msg, tx);
        self.sender
            .send(ActorMessage::Async(Box::new(envelope)))
            .map_err(|_| MailboxError::MailboxClosed)?;
        rx.await.map_err(|_| MailboxError::MailboxClosed)
    }

    ///Check if the actor is still alive
    pub fn is_alive(&self) -> bool {
        !self.sender.is_closed()
    }

    ///register a watcher to be notified when this actor stops
    /// the watcher will receive a Terminated message with this actor's id
    pub fn watch<W>(&self, watcher: Addr<W>)
    where
        W: Actor + Handler<Terminated>,
    {
        let watcher_arc = Arc::new(watcher);
        self.watchers.lock().unwrap().push(watcher_arc);
    }

    pub(crate) fn notify_watchers(&self) {
        let watchers = self.watchers.lock().unwrap();
        for watcher in watchers.iter() {
            watcher.notify(self.id);
        }
    }
}

impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            id: self.id,
            watchers: self.watchers.clone(),
        }
    }
}

impl<A> Watcher for Addr<A>
where
    A: Actor + Handler<Terminated>,
{
    fn notify(&self, id: ActorId) {
        self.do_send(Terminated { id });
    }
}
