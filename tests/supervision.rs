use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use cinema::{message::Terminated, Actor, ActorSystem, Addr, Context, Handler, Message};

struct Crash;

impl Message for Crash {
    type Result = ();
}

struct Ping;
impl Message for Ping {
    type Result = ();
}

struct CrashActor {
    stop_called: Arc<AtomicBool>,
}

impl Actor for CrashActor {
    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        self.stop_called.store(true, Ordering::SeqCst);
    }
}

impl Handler<Crash> for CrashActor {
    fn handle(&mut self, _msg: Crash, _ctx: &mut Context<Self>) {
        panic!("Intentional crash!");
    }
}
impl Handler<Ping> for CrashActor {
    fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self>) {
        // Do nothing
    }
}

#[tokio::test]
async fn actor_panic_stops_gracefully() {
    let stopped_called = Arc::new(AtomicBool::new(false));
    let actor = CrashActor {
        stop_called: stopped_called.clone(),
    };

    let sys = ActorSystem::new();
    let addr = sys.spawn(actor);

    tokio::time::sleep(Duration::from_millis(10)).await;

    addr.do_send(Crash);

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(stopped_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn actor_continues_after_normal_messages() {
    let stopped_called = Arc::new(AtomicBool::new(false));
    let actor = CrashActor {
        stop_called: stopped_called.clone(),
    };

    let sys = ActorSystem::new();
    let addr = sys.spawn(actor);

    addr.do_send(Ping);
    addr.do_send(Ping);
    addr.do_send(Ping);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(!stopped_called.load(Ordering::SeqCst));

    sys.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(stopped_called.load(Ordering::SeqCst));
}

struct Die;
impl Message for Die {
    type Result = ();
}

struct Worker;
impl Actor for Worker {}

impl Handler<Die> for Worker {
    fn handle(&mut self, _msg: Die, ctx: &mut Context<Self>) {
        ctx.stop();
    }
}

struct Monitor {
    worker_addr: Option<Addr<Worker>>,
    worker_died: Arc<AtomicBool>,
}

struct SetWorker(Addr<Worker>);
impl Message for SetWorker {
    type Result = ();
}

impl Actor for Monitor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Some(ref worker_addr) = self.worker_addr {
            ctx.watch(worker_addr);
        }
    }
}

impl Handler<SetWorker> for Monitor {
    fn handle(&mut self, msg: SetWorker, ctx: &mut Context<Self>) {
        self.worker_addr = Some(msg.0);
        ctx.watch(self.worker_addr.as_ref().unwrap());
    }
}

impl Handler<Terminated> for Monitor {
    fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {
        self.worker_died.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn watch_notifies_on_death() {
    let worker_died = Arc::new(AtomicBool::new(false));

    let sys = ActorSystem::new();

    let worker_addr = sys.spawn(Worker);

    let monitor = Monitor {
        worker_addr: Some(worker_addr.clone()),
        worker_died: worker_died.clone(),
    };

    let _monitor_addr = sys.spawn(monitor);

    tokio::time::sleep(Duration::from_millis(10)).await;

    worker_addr.do_send(Die);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(worker_died.load(Ordering::SeqCst));
}
