use tokio::sync::mpsc;

use crate::{
    envelope::{Envelope, MessageEnvelope},
    Actor, Handler, Message,
};

pub struct Addr<A: Actor> {
    sender: mpsc::UnboundedSender<Box<dyn Envelope<A>>>,
}

impl<A: Actor> Addr<A> {
    pub fn new(sender: mpsc::UnboundedSender<Box<dyn Envelope<A>>>) -> Self {
        Self { sender }
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
