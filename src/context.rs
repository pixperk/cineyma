use std::{sync::Arc, time::Duration};

use tokio::sync::Notify;

use crate::{actor::ActorId, message::Terminated, Actor, Addr, Handler, Message, TimerHandle};

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

    /// Get this actor's ID
    pub fn id(&self) -> ActorId {
        self.addr.id()
    }

    ///stop the actor associated with this context
    pub fn stop(&self) {
        if let Some(signal) = &self.stop_signal {
            signal.notify_one();
        }
    }

    /// Watch another actor - receive Terminated when it dies
    /// When the watched actor stops, this actor will receive
    /// a Terminated message with the dead actor's ID
    pub fn watch<B>(&self, addr: &Addr<B>)
    where
        B: Actor,
        A: Handler<Terminated>,
    {
        addr.watch(self.addr.clone());
    }

    /// Send a message to self after delay
    /// Returns a TimerHandle that can be used to cancel the timer
    pub fn run_later<M>(&self, delay: Duration, msg: M) -> TimerHandle
    where
        M: Message,
        A: Handler<M>,
    {
        let addr = self.addr.clone();
        let handle = TimerHandle::new();
        let handle_clone = handle.clone();

        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if !handle_clone.is_cancelled() {
                addr.do_send(msg);
            }
        });

        handle
    }

    /// Send a message to self repeatedly at fixed intervals
    /// Returns a TimerHandle that can be used to cancel the interval
    pub fn run_interval<M>(&self, interval: Duration, msg: M) -> TimerHandle
    where
        M: Message + Clone,
        A: Handler<M>,
    {
        let addr = self.addr.clone();
        let handle = TimerHandle::new();
        let handle_clone = handle.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if !addr.is_alive() || handle_clone.is_cancelled() {
                    break;
                }
                addr.do_send(msg.clone());
            }
        });

        handle
    }
}
