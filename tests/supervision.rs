use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use cineyma::{
    address::ChildHandle, message::Terminated, Actor, ActorSystem, Addr, Context, Handler, Message,
    SupervisorStrategy,
};

// ======== Panic Handling Tests ========

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

    addr.do_send(Crash).await.unwrap();

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

    addr.do_send(Ping).await.unwrap();
    addr.do_send(Ping).await.unwrap();
    addr.do_send(Ping).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(!stopped_called.load(Ordering::SeqCst));

    sys.shutdown();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(stopped_called.load(Ordering::SeqCst));
}

// ======== Death Watch Tests ========

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

    worker_addr.do_send(Die).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(worker_died.load(Ordering::SeqCst));
}

// ======== Child Actor Tests ========

///parent stopping kills child actors
#[tokio::test]
async fn parent_stopping_stops_children() {
    static CHILD_STOPPED: AtomicBool = AtomicBool::new(false);

    struct Child;
    impl Actor for Child {
        fn stopped(&mut self, _ctx: &mut Context<Self>) {
            CHILD_STOPPED.store(true, Ordering::SeqCst);
        }
    }

    struct Parent {
        child_addr: Option<Addr<Child>>,
    }

    impl Handler<Terminated> for Parent {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {}
    }

    impl Actor for Parent {
        fn started(&mut self, ctx: &mut Context<Self>) {
            self.child_addr = Some(ctx.spawn_child(Child));
        }
    }

    let sys = ActorSystem::new();
    let parent_addr = sys.spawn(Parent { child_addr: None });

    tokio::time::sleep(Duration::from_millis(10)).await;

    parent_addr.stop();

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(CHILD_STOPPED.load(Ordering::SeqCst));
}

//child dies, parent gets Terminated message
#[tokio::test]
async fn child_stopping_notifies_parent() {
    static PARENT_NOTIFIED: AtomicBool = AtomicBool::new(false);

    struct Child;
    impl Actor for Child {}

    struct DieMsg;
    impl Message for DieMsg {
        type Result = ();
    }

    impl Handler<DieMsg> for Child {
        fn handle(&mut self, _msg: DieMsg, ctx: &mut Context<Self>) {
            ctx.stop();
        }
    }

    struct Parent {
        child_addr: Option<Addr<Child>>,
    }

    impl Actor for Parent {
        fn started(&mut self, ctx: &mut Context<Self>) {
            let child_addr = ctx.spawn_child(Child);
            self.child_addr = Some(child_addr);
        }
    }

    impl Handler<Terminated> for Parent {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {
            PARENT_NOTIFIED.store(true, Ordering::SeqCst);
        }
    }

    // Message to trigger child stop
    struct StopChild;
    impl Message for StopChild {
        type Result = ();
    }
    impl Handler<StopChild> for Parent {
        fn handle(&mut self, _msg: StopChild, _ctx: &mut Context<Self>) {
            if let Some(child) = &self.child_addr {
                let _ = child.try_send(DieMsg);
            }
        }
    }

    let sys = ActorSystem::new();
    let parent = sys.spawn(Parent { child_addr: None });

    tokio::time::sleep(Duration::from_millis(50)).await;

    parent.do_send(StopChild).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        PARENT_NOTIFIED.load(Ordering::SeqCst),
        "Parent should receive Terminated"
    );
}

// ======== Restart Strategy Tests ========
///actor restarts on panic according to strategy
#[tokio::test]
async fn actor_restarts_on_panic() {
    static RESTART_COUNT: AtomicU32 = AtomicU32::new(0);

    struct RestartableWorker;

    impl Actor for RestartableWorker {
        fn started(&mut self, _ctx: &mut Context<Self>) {
            RESTART_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct CrashMsg;
    impl Message for CrashMsg {
        type Result = ();
    }

    impl Handler<CrashMsg> for RestartableWorker {
        fn handle(&mut self, _msg: CrashMsg, _ctx: &mut Context<Self>) {
            panic!("Intentional crash for restart test");
        }
    }

    struct Supervisor;
    impl Actor for Supervisor {}

    impl Handler<Terminated> for Supervisor {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {}
    }

    struct SpawnWorker;
    impl Message for SpawnWorker {
        type Result = Addr<RestartableWorker>;
    }

    impl Handler<SpawnWorker> for Supervisor {
        fn handle(
            &mut self,
            _msg: SpawnWorker,
            ctx: &mut Context<Self>,
        ) -> Addr<RestartableWorker> {
            ctx.spawn_child_with_strategy(
                || RestartableWorker,
                SupervisorStrategy::restart(5, Duration::from_secs(10)),
            )
        }
    }

    let sys = ActorSystem::new();
    let supervisor = sys.spawn(Supervisor);

    let worker = supervisor.send(SpawnWorker).await.unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(RESTART_COUNT.load(Ordering::SeqCst), 1);

    for i in 2..=4 {
        worker.do_send(CrashMsg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(RESTART_COUNT.load(Ordering::SeqCst), i);
    }
}

///actor stops after exceeding max restarts
#[tokio::test]
async fn actor_stops_after_max_restarts() {
    static RESTART_COUNT: AtomicU32 = AtomicU32::new(0);
    static TERMINATED_RECEIVED: AtomicBool = AtomicBool::new(false);

    struct FragileWorker;
    impl Actor for FragileWorker {
        fn started(&mut self, _ctx: &mut Context<Self>) {
            RESTART_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct CrashMsg;
    impl Message for CrashMsg {
        type Result = ();
    }

    impl Handler<CrashMsg> for FragileWorker {
        fn handle(&mut self, _msg: CrashMsg, _ctx: &mut Context<Self>) {
            panic!("Intentional crash for restart limit test");
        }
    }

    struct Supervisor;
    impl Actor for Supervisor {}
    impl Handler<Terminated> for Supervisor {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {
            TERMINATED_RECEIVED.store(true, Ordering::SeqCst);
        }
    }

    struct SpawnFragileWorker;
    impl Message for SpawnFragileWorker {
        type Result = Addr<FragileWorker>;
    }

    impl Handler<SpawnFragileWorker> for Supervisor {
        fn handle(
            &mut self,
            _msg: SpawnFragileWorker,
            ctx: &mut Context<Self>,
        ) -> Addr<FragileWorker> {
            ctx.spawn_child_with_strategy(
                || FragileWorker,
                SupervisorStrategy::restart(2, Duration::from_secs(10)),
            )
        }
    }

    let sys = ActorSystem::new();
    let supervisor = sys.spawn(Supervisor);

    let worker = supervisor.send(SpawnFragileWorker).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Crash 3 times - limit is 2
    worker.do_send(CrashMsg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    worker.do_send(CrashMsg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    worker.do_send(CrashMsg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should have started 3 times (initial + 2 restarts), then stopped
    assert_eq!(RESTART_COUNT.load(Ordering::SeqCst), 3);
    assert!(
        TERMINATED_RECEIVED.load(Ordering::SeqCst),
        "Supervisor should receive Terminated"
    );
    assert!(!worker.is_alive(), "Worker should be dead");
}

// ======== Escalate Strategy Tests ========

/// child panic causes parent to "panic",
/// which triggers grandparent's restart strategy
#[tokio::test]
async fn escalate_triggers_parent_restart() {
    // Track how many times Parent gets started (should be 2: initial + 1 restart)
    static PARENT_START_COUNT: AtomicU32 = AtomicU32::new(0);
    // Track how many times Child gets started
    static CHILD_START_COUNT: AtomicU32 = AtomicU32::new(0);

    // Grandchild that panics
    struct Grandchild;
    impl Actor for Grandchild {
        fn started(&mut self, _ctx: &mut Context<Self>) {
            CHILD_START_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct CrashMsg;
    impl Message for CrashMsg {
        type Result = ();
    }

    impl Handler<CrashMsg> for Grandchild {
        fn handle(&mut self, _msg: CrashMsg, _ctx: &mut Context<Self>) {
            panic!("Grandchild crash to test escalate!");
        }
    }

    // Parent spawns Grandchild with Escalate strategy
    struct Parent {
        grandchild: Option<Addr<Grandchild>>,
    }

    impl Actor for Parent {
        fn started(&mut self, ctx: &mut Context<Self>) {
            PARENT_START_COUNT.fetch_add(1, Ordering::SeqCst);
            // Parent spawns grandchild with Escalate strategy
            self.grandchild =
                Some(ctx.spawn_child_with_strategy(|| Grandchild, SupervisorStrategy::Escalate));
        }
    }

    impl Handler<Terminated> for Parent {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {}
    }

    // Message to crash the grandchild via parent
    struct CrashGrandchild;
    impl Message for CrashGrandchild {
        type Result = ();
    }

    impl Handler<CrashGrandchild> for Parent {
        fn handle(&mut self, _msg: CrashGrandchild, _ctx: &mut Context<Self>) {
            if let Some(ref gc) = self.grandchild {
                let _ = gc.try_send(CrashMsg);
            }
        }
    }

    // Grandparent spawns Parent with Restart strategy
    struct Grandparent;
    impl Actor for Grandparent {}

    impl Handler<Terminated> for Grandparent {
        fn handle(&mut self, _msg: Terminated, _ctx: &mut Context<Self>) {}
    }

    struct SpawnParent;
    impl Message for SpawnParent {
        type Result = Addr<Parent>;
    }

    impl Handler<SpawnParent> for Grandparent {
        fn handle(&mut self, _msg: SpawnParent, ctx: &mut Context<Self>) -> Addr<Parent> {
            ctx.spawn_child_with_strategy(
                || Parent { grandchild: None },
                SupervisorStrategy::restart(3, Duration::from_secs(10)),
            )
        }
    }

    let sys = ActorSystem::new();
    let grandparent = sys.spawn(Grandparent);

    let parent = grandparent.send(SpawnParent).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        PARENT_START_COUNT.load(Ordering::SeqCst),
        1,
        "Parent should have started once"
    );
    assert_eq!(
        CHILD_START_COUNT.load(Ordering::SeqCst),
        1,
        "Grandchild should have started once"
    );

    parent.do_send(CrashGrandchild).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(
        PARENT_START_COUNT.load(Ordering::SeqCst),
        2,
        "Parent should have been restarted by grandparent"
    );

    assert_eq!(
        CHILD_START_COUNT.load(Ordering::SeqCst),
        2,
        "Grandchild should have been recreated with new parent"
    );
}
