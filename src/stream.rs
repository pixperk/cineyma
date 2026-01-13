use core::task;
use std::{pin::Pin, task::Poll};

use futures::Stream;

use crate::{actor::StreamHandler, Actor, Context};

///type erased stream that can call back into actor
pub trait ActorStream<A: Actor>: Send {
    fn poll_next(
        self: Pin<&mut Self>,
        actor: &mut A,
        ctx: &mut Context<A>,
        task_ctx: &mut task::Context<'_>,
    ) -> Poll<bool>;
}

pub struct StreamWrapper<S, I>
where
    S: Stream<Item = I> + Send + Unpin,
    I: Send + 'static,
{
    stream: S,
    _phantom: std::marker::PhantomData<I>,
}

impl<A, S, I> ActorStream<A> for StreamWrapper<S, I>
where
    A: Actor + StreamHandler<I>,
    S: Stream<Item = I> + Send + Unpin,
    I: Send + 'static + Unpin,
{
    fn poll_next(
        mut self: Pin<&mut Self>,
        actor: &mut A,
        ctx: &mut Context<A>,
        task_ctx: &mut task::Context<'_>,
    ) -> Poll<bool> {
        match Pin::new(&mut self.as_mut().get_mut().stream).poll_next(task_ctx) {
            Poll::Ready(Some(item)) => {
                actor.handle(item, ctx);
                Poll::Ready(true)
            }
            Poll::Ready(None) => {
                actor.finished(ctx);
                Poll::Ready(false)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S, I> StreamWrapper<S, I>
where
    S: Stream<Item = I> + Send + Unpin,
    I: Send + 'static,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Poll all streams once, remove finished ones
/// Returns true if any stream produced an item (handler was called)
pub fn poll_streams<A: Actor>(
    streams: &mut Vec<Pin<Box<dyn ActorStream<A>>>>,
    actor: &mut A,
    ctx: &mut Context<A>,
    task_ctx: &mut task::Context<'_>,
) -> bool {
    let mut any_ready = false;

    streams.retain_mut(|stream| {
        match stream.as_mut().poll_next(actor, ctx, task_ctx) {
            Poll::Ready(true) => {
                any_ready = true;
                true // keep stream, might have more items
            }
            Poll::Ready(false) => false, // stream finished, remove it
            Poll::Pending => true,       // no item yet, keep stream
        }
    });

    any_ready
}
