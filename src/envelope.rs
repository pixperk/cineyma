use crate::{Actor, Context, Handler, Message};

///Envelope acts as a type erasure for messages sent to actors
/// it is wrapped in a Box to allow for dynamic dispatch
pub trait Envelope<A: Actor>: Send {
    fn handle(self: Box<Self>, actor: &mut A, ctx: &mut Context<A>);
}

pub struct MessageEnvelope<M>
where
    M: Message,
{
    //an optional message, once taken it becomes None
    pub msg: Option<M>,
}

impl<M: Message> MessageEnvelope<M> {
    pub fn new(msg: M) -> Self {
        Self { msg: Some(msg) }
    }
}

impl<A, M> Envelope<A> for MessageEnvelope<M>
where
    A: Actor + Handler<M>,
    M: Message,
{
    fn handle(mut self: Box<Self>, actor: &mut A, ctx: &mut Context<A>) {
        if let Some(msg) = self.msg.take() {
            actor.handle(msg, ctx);
            //ignore the result for now
        }
    }
}
