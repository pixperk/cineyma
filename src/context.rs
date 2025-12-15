use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures::{FutureExt, Stream};
use tokio::sync::{mpsc, Notify};

use crate::{
    actor::{ActorId, StreamHandler},
    address::ChildHandle,
    envelope::ActorMessage,
    message::Terminated,
    stream::{ActorStream, StreamWrapper},
    supervisor::RestartTracker,
    Actor, Addr, Handler, Message, SupervisorStrategy, TimerHandle,
};

///Runtime context for an actor
pub struct Context<A: Actor> {
    addr: Addr<A>,
    ///signal to stop the actor
    stop_signal: Option<Arc<Notify>>,
    shutdown: Arc<Notify>,
    children: Vec<Box<dyn ChildHandle>>,
    escalate_signal: Arc<Notify>,
    streams: Vec<Pin<Box<dyn ActorStream<A>>>>,
}

impl<A: Actor> Context<A> {
    pub fn new(addr: Addr<A>, shutdown: Arc<Notify>) -> Self {
        Self {
            addr,
            stop_signal: None,
            shutdown,
            children: Vec::new(),
            escalate_signal: Arc::new(Notify::new()),
            streams: Vec::new(),
        }
    }

    ///configure the context with a stop signal for graceful shutdown
    pub fn with_stop_signal(
        addr: Addr<A>,
        stop_signal: Arc<Notify>,
        shutdown: Arc<Notify>,
    ) -> Self {
        Self {
            addr,
            stop_signal: Some(stop_signal),
            shutdown,
            children: Vec::new(),
            escalate_signal: Arc::new(Notify::new()),
            streams: Vec::new(),
        }
    }

    ///configure the context with custom signals
    pub fn with_signals(
        addr: Addr<A>,
        stop_signal: Arc<Notify>,
        shutdown: Arc<Notify>,
        escalate_signal: Arc<Notify>,
    ) -> Self {
        Self {
            addr,
            stop_signal: Some(stop_signal),
            shutdown,
            children: Vec::new(),
            escalate_signal,
            streams: Vec::new(),
        }
    }

    ///Get the escalate signal for this actor
    pub fn escalate_signal(&self) -> Arc<Notify> {
        self.escalate_signal.clone()
    }

    ///Stop all child actors (when this actor stops)
    pub fn stop_children(&mut self) {
        for child in &self.children {
            child.stop();
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
        addr.add_watcher(self.addr.clone());
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

    ///Spawn a child actor supervised by this actor
    /// Child inherits shutdown signal from parent
    /// Stops when parent stops
    /// Parent receives Terminated message when child stops
    pub fn spawn_child<C>(&mut self, child: C) -> Addr<C>
    where
        C: Actor,
        A: Handler<Terminated>,
    {
        let mut child_opt = Some(child);
        self.spawn_child_with_strategy(
            move || child_opt.take().expect("Factory called more than once"),
            SupervisorStrategy::Stop,
        )
    }

    /// Spawn a child with custom restart strategy
    pub fn spawn_child_with_strategy<C, F>(
        &mut self,
        mut factory: F,
        strategy: SupervisorStrategy,
    ) -> Addr<C>
    where
        C: Actor,
        A: Handler<Terminated>,
        F: FnMut() -> C + Send + 'static,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<ActorMessage<C>>();
        let child_id = ActorId::new();
        let child_stop_signal = Arc::new(Notify::new());
        let child_addr = Addr::new(tx, child_id, child_stop_signal.clone());

        let shutdown = self.shutdown.clone();
        let child_addr_for_notify = child_addr.clone();

        let parent_escalate_signal = self.escalate_signal.clone();

        tokio::spawn(async move {
            let mut tracker = match &strategy {
                SupervisorStrategy::Restart {
                    max_restarts,
                    within,
                } => Some(RestartTracker::new(*max_restarts, *within)),
                _ => None,
            };

            'restart: loop {
                let mut child = factory();
                let mut child_ctx = Context::with_stop_signal(
                    child_addr_for_notify.clone(),
                    child_stop_signal.clone(),
                    shutdown.clone(),
                );

                child.started(&mut child_ctx);

                let child_escalate_signal = child_ctx.escalate_signal();

                let panic_occurred = loop {
                    tokio::select! {
                                    msg = rx.recv() => {
                                        match msg {
                                            Some(actor_msg) => {
                                                let result = match actor_msg {
                                                    ActorMessage::Sync(envelope) => {
                                                        catch_unwind(AssertUnwindSafe(|| {
                                                            envelope.handle(&mut child, &mut child_ctx)
                                                        }))
                                                    }
                                                    ActorMessage::Async(envelope) => {
                                                        let fut = envelope.handle(&mut child, &mut child_ctx);
                                                        AssertUnwindSafe(fut).catch_unwind().await
                                                    }
                                                };
                                                if result.is_err() {
                                                    break true;
                                                }
                                            }
                                            None => break false,
                                        }
                                    }
                                    _ = shutdown.notified() => break false,
                                    _ = child_stop_signal.notified() => break false,
                                    _ = child_escalate_signal.notified() => {
                        eprintln!("Child received escalate from grandchild.");
                        break true;
                    }
                                }
                };

                child_ctx.stop_children();
                child.stopped(&mut child_ctx);

                if panic_occurred {
                    match &strategy {
                        SupervisorStrategy::Stop => {
                            eprintln!("Child panicked. Strategy: Stop.");
                            break 'restart;
                        }
                        SupervisorStrategy::Restart { .. } => {
                            if let Some(ref mut t) = tracker {
                                if t.record_restart() {
                                    eprintln!("Child panicked. Restarting...");
                                    continue 'restart;
                                } else {
                                    eprintln!("Child exceeded restart limit. Stopping.");
                                    break 'restart;
                                }
                            }
                        }
                        SupervisorStrategy::Escalate => {
                            eprintln!("Child panicked. Strategy: Escalate. Notifying parent.");
                            parent_escalate_signal.notify_one();
                            break 'restart;
                        }
                    }
                } else {
                    break 'restart;
                }
            }

            child_addr_for_notify.notify_watchers();
        });

        //auto watch the child
        self.watch(&child_addr);

        //keep track of child for stopping later
        self.children.push(Box::new(child_addr.clone()));

        child_addr
    }

    /// Add a stream to be handled by this actor
    pub fn add_stream<S, I>(&mut self, stream: S)
    where
        S: Stream<Item = I> + Send + Unpin + 'static,
        I: Send + 'static + Unpin,
        A: StreamHandler<I>,
    {
        let wrapper = StreamWrapper::new(stream);
        self.streams.push(Box::pin(wrapper));
    }

    /// Take the streams out of the context (avoids borrow issues)
    pub fn take_streams(&mut self) -> Vec<Pin<Box<dyn ActorStream<A>>>> {
        std::mem::take(&mut self.streams)
    }

    /// Return streams back to the context, and add new ones if any
    pub fn return_streams(&mut self, mut streams: Vec<Pin<Box<dyn ActorStream<A>>>>) {
        streams.append(&mut self.streams);
        self.streams = streams;
    }
}
