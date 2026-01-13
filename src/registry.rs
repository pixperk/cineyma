use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{Actor, Addr};

pub struct Registry {
    actors: RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self {
            actors: RwLock::new(HashMap::new()),
        }
    }

    /// Register actor with automatic unregistration on death (default)
    pub fn register<A: Actor>(registry: Arc<Self>, name: &str, addr: Addr<A>) {
        // Insert into registry
        {
            let mut map = match registry.actors.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            map.insert(name.to_string(), Box::new(addr.clone()));
        }

        // Spawn watcher task for auto-unregister
        let name = name.to_string();
        let addr_watch = addr;
        tokio::spawn(async move {
            while addr_watch.is_alive() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            registry.unregister(&name);
        });
    }

    /// Register actor without auto-unregister (manual cleanup required)
    pub fn register_manual<A: Actor>(&self, name: &str, addr: Addr<A>) {
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
        map.get(name)
            .and_then(|boxed| boxed.downcast_ref::<Addr<A>>())
            .cloned()
    }

    pub fn unregister(&self, name: &str) {
        let mut map = match self.actors.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.remove(name);
    }
}
