use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use cineyma::{Actor, Handler, Message};

// ======== Actor Lifecycle Tests ========

struct StopMe;

impl Message for StopMe {
    type Result = ();
}

struct TestActor {
    stopped: Arc<AtomicBool>,
}

impl Actor for TestActor {
    fn stopped(&mut self, _ctx: &mut cineyma::Context<Self>) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

impl Handler<StopMe> for TestActor {
    fn handle(&mut self, _msg: StopMe, ctx: &mut cineyma::Context<Self>) {
        ctx.stop();
    }
}

#[tokio::test]
async fn actor_stops_itself() {
    let stopped = Arc::new(AtomicBool::new(false));
    let actor = TestActor {
        stopped: stopped.clone(),
    };

    let system = cineyma::system::ActorSystem::new();
    let addr = system.spawn(actor);

    //give some time for the actor to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    addr.do_send(StopMe).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn system_shutdown_stops_actor() {
    let stopped = Arc::new(AtomicBool::new(false));
    let actor = TestActor {
        stopped: stopped.clone(),
    };

    let sys = cineyma::system::ActorSystem::new();
    let _addr = sys.spawn(actor);

    //give some time for the actor to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    sys.shutdown();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(stopped.load(Ordering::SeqCst));
}

// ======== Actor Registry Tests ========

struct RegistryActor;

impl Actor for RegistryActor {}

impl Handler<StopMe> for RegistryActor {
    fn handle(&mut self, _msg: StopMe, ctx: &mut cineyma::Context<Self>) {
        ctx.stop();
    }
}

#[tokio::test]
async fn registry_register_and_lookup() {
    let sys = cineyma::ActorSystem::new();
    let addr = sys.spawn(RegistryActor);

    sys.register("my_actor", addr.clone());

    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("my_actor");
    assert!(found.is_some());
}

#[tokio::test]
async fn registry_lookup_nonexistent_returns_none() {
    let sys = cineyma::ActorSystem::new();

    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("does_not_exist");
    assert!(found.is_none());
}

#[tokio::test]
async fn registry_auto_unregisters_on_actor_death() {
    let sys = cineyma::ActorSystem::new();
    let addr = sys.spawn(RegistryActor);

    sys.register("dying_actor", addr.clone());

    // Actor is alive, should be found
    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("dying_actor");
    assert!(found.is_some());

    // Stop the actor
    addr.do_send(StopMe).await.unwrap();

    // Wait for actor to die and auto-unregister (50ms poll + some buffer)
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Should be unregistered now
    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("dying_actor");
    assert!(
        found.is_none(),
        "Actor should be auto-unregistered after death"
    );
}

#[tokio::test]
async fn registry_manual_does_not_auto_unregister() {
    let sys = cineyma::ActorSystem::new();
    let addr = sys.spawn(RegistryActor);

    sys.register_manual("manual_actor", addr.clone());

    // Stop the actor
    addr.do_send(StopMe).await.unwrap();

    // Wait some time
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Should still be registered (manual = no auto-unregister)
    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("manual_actor");
    assert!(
        found.is_some(),
        "Manual registration should not auto-unregister"
    );

    // Manual cleanup
    sys.unregister("manual_actor");
    let found: Option<cineyma::Addr<RegistryActor>> = sys.lookup("manual_actor");
    assert!(found.is_none());
}
