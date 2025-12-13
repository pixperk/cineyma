use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use cinema::{Actor, Handler, Message};

struct StopMe;

impl Message for StopMe {
    type Result = ();
}

struct TestActor {
    stopped: Arc<AtomicBool>,
}

impl Actor for TestActor {
    fn stopped(&mut self, _ctx: &mut cinema::Context<Self>) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

impl Handler<StopMe> for TestActor {
    fn handle(&mut self, _msg: StopMe, ctx: &mut cinema::Context<Self>) {
        ctx.stop();
    }
}

#[tokio::test]
async fn actor_stops_itself() {
    let stopped = Arc::new(AtomicBool::new(false));
    let actor = TestActor {
        stopped: stopped.clone(),
    };

    let system = cinema::system::ActorSystem::new();
    let addr = system.spawn(actor);

    //give some time for the actor to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    addr.do_send(StopMe);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn system_shutdown_stops_actor() {
    let stopped = Arc::new(AtomicBool::new(false));
    let actor = TestActor {
        stopped: stopped.clone(),
    };

    let sys = cinema::system::ActorSystem::new();
    let _addr = sys.spawn(actor);

    //give some time for the actor to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    sys.shutdown();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(stopped.load(Ordering::SeqCst));
}
