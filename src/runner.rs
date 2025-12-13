use tokio::sync::mpsc;

use crate::{envelope::Envelope, Actor, Addr};

pub fn spawn<A>(mut actor: A) -> Addr<A>
where
    A: Actor,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<Box<dyn Envelope<A>>>();
    let addr = Addr::new(tx);
    let mut ctx = crate::context::Context::new(addr.clone());

    tokio::spawn(async move {
        //actor lifecycle start
        actor.started(&mut ctx);

        //message processing loop
        while let Some(envelope) = rx.recv().await {
            envelope.handle(&mut actor, &mut ctx);
        }

        //actor lifecycle stop
        actor.stopped(&mut ctx);
    });

    addr
}
