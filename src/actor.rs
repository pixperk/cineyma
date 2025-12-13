use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Context, Message};

//it is an entity which has own state, also
//it's size is to be known during compile time
pub trait Actor: Send + Sized + 'static {
    fn started(&mut self, ctx: &mut Context<Self>) {}
    fn stopped(&mut self, ctx: &mut Context<Self>) {}
}

/// Unique identifier for an actor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActorId(u64);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl ActorId {
    pub fn new() -> Self {
        Self(NEXT_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

/// Defines how an actor handles a specific message type.
/// One actor can handle multiple message types
pub trait Handler<M: Message>: Actor {
    fn handle(&mut self, msg: M, ctx: &mut Context<Self>) -> M::Result;
}

///return type for async functions in actors
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

///async version of Handler trait
pub trait AsyncHandler<M: Message>: Actor {
    fn handle(&mut self, msg: M, ctx: &mut Context<Self>) -> BoxFuture<'_, M::Result>;
}
