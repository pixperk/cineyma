use crate::{Actor, Addr};

///Runtime context for an actor
pub struct Context<A: Actor> {
    addr: Addr<A>,
}

impl<A: Actor> Context<A> {
    pub fn new(addr: Addr<A>) -> Self {
        Self { addr }
    }

    ///Get the address of the actor associated with this context
    pub fn address(&self) -> Addr<A> {
        self.addr.clone()
    }
}
