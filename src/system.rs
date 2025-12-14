use std::sync::Arc;

use futures::FutureExt;
use tokio::sync::{mpsc, Notify};

use crate::{actor::ActorId, envelope::ActorMessage, Actor, Addr, Context};

use std::panic::{catch_unwind, AssertUnwindSafe};

///Actor system for managing actors and their lifecycle
pub struct ActorSystem {
    //shared notify for graceful shutdown
    shutdown: Arc<Notify>,
}

impl ActorSystem {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(Notify::new()),
        }
    }

    //spawn a top-level actor
    pub fn spawn<A>(&self, actor: A) -> Addr<A>
    where
        A: Actor,
    {
        spawn_with_shutdown(actor, self.shutdown.clone())
    }

    //gracefully shutdown the actor system
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

impl Default for ActorSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_with_shutdown<A>(mut actor: A, shutdown: Arc<Notify>) -> Addr<A>
where
    A: Actor,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<ActorMessage<A>>();
    let id = ActorId::new();

    let stop_signal = Arc::new(Notify::new());

    let addr = Addr::new(tx, id, stop_signal.clone());

    let mut ctx = Context::with_stop_signal(addr.clone(), stop_signal.clone(), shutdown.clone());

    let addr_for_notify = addr.clone();

    tokio::spawn(async move {
        //actor lifecycle start
        actor.started(&mut ctx);

        let escalate_signal = ctx.escalate_signal();

        let panic_occured = loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(actor_msg) => {
                            let result = match actor_msg {
                                ActorMessage::Sync(envelope) => {
                                    catch_unwind(AssertUnwindSafe(|| {
                                        envelope.handle(&mut actor, &mut ctx)
                                    }))
                                }
                                ActorMessage::Async(envelope) => {
                                    let fut = envelope.handle(&mut actor, &mut ctx);
                                    AssertUnwindSafe(fut).catch_unwind().await
                                }
                            };
                            if result.is_err() {
                                break true;
                            }
                        }
                        None => {
                            break false;
                        }
                    }
                }
                _ = shutdown.notified() => {
                    break false;
                }
                _ = stop_signal.notified() => {
                    break false;
                }
                _ = escalate_signal.notified() => {
                    //escalation requested, we treat it as panic for top-level actors
                    eprintln!("Actor received escalation signal. Treating as panic.");
                    break true;
                }
            }
        };

        if panic_occured {
            //actor panicked, we can log or handle it here
            eprintln!("Actor panicked during message handling. Stopping gracefully.");
        }

        //notify watchers about termination
        addr_for_notify.notify_watchers();

        //stop all child actors
        ctx.stop_children();

        //actor lifecycle stop
        actor.stopped(&mut ctx);
    });

    addr
}
