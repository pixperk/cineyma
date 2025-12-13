use std::sync::Arc;

use tokio::sync::Notify;

use crate::{Actor, Addr};

///Runtime context for an actor
pub struct Context<A: Actor> {
    addr: Addr<A>,
    ///signal to stop the actor
    stop_signal: Option<Arc<Notify>>,
}

impl<A: Actor> Context<A> {
    pub fn new(addr: Addr<A>) -> Self {
        Self {
            addr,
            stop_signal: None,
        }
    }

    ///configure the context with a stop signal for graceful shutdown
    pub fn with_stop_signal(addr: Addr<A>, stop_signal: Arc<Notify>) -> Self {
        Self {
            addr,
            stop_signal: Some(stop_signal),
        }
    }

    ///Get the address of the actor associated with this context
    pub fn address(&self) -> Addr<A> {
        self.addr.clone()
    }

    ///stop the actor associated with this context
    pub fn stop(&self) {
        if let Some(signal) = &self.stop_signal {
            signal.notify_one();
        }
    }
}
