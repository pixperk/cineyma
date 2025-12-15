use std::{any::Any, collections::HashMap, sync::RwLock};

use crate::{Actor, Addr};

pub struct Registry {
    actors: RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            actors: RwLock::new(HashMap::new()),
        }
    }

    pub fn register<A: Actor>(&self, name: &str, addr: Addr<A>) {
        let mut map = match self.actors.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.insert(name.to_string(), Box::new(addr));
    }

    pub fn lookup<A: Actor>(&self, name: &str) -> Option<Addr<A>> {
        let map = match self.actors.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.get(name).and_then(|boxed_addr| {
            boxed_addr
                .downcast_ref::<Addr<A>>()
                .map(|addr| addr.clone())
        })
    }

    pub fn unregister(&self, name: &str) {
        let mut map = match self.actors.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.remove(name);
    }
}
