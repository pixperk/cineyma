use std::sync::Arc;

use tokio::sync::{mpsc, Notify};

use crate::{envelope::Envelope, Actor, Addr, Context};

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
    let (tx, mut rx) = mpsc::unbounded_channel::<Box<dyn Envelope<A>>>();
    let addr = Addr::new(tx);
    let stop_signal = Arc::new(Notify::new());
    let mut ctx = Context::with_stop_signal(addr.clone(), stop_signal.clone());

    tokio::spawn(async move {
        //actor lifecycle start
        actor.started(&mut ctx);

        loop {
            tokio::select! {
                //message processing loop
                msg = rx.recv() => {
                    match msg{
                        Some(envelope) => {
                            envelope.handle(&mut actor, &mut ctx);
                        },
                        None => {
                            //channel closed, exit loop
                            break;
                        }
                    }
                }

                _ = shutdown.notified() => {
                    //shutdown signal received, exit loop
                    break;
                }
                _ = stop_signal.notified() => {
                    //stop signal received, exit loop
                    break;
                }
            }
        }

        //actor lifecycle stop
        actor.stopped(&mut ctx);
    });

    addr
}
