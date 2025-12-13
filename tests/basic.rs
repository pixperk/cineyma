use std::{
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use cinema::{Actor, ActorSystem, Context, Handler, MailboxError, Message, TimerHandle};

struct Ping;
impl Message for Ping {
    type Result = ();
}

struct PingActor {
    count: Arc<AtomicUsize>,
}

impl Actor for PingActor {
    fn started(&mut self, _ctx: &mut Context<Self>) {
        println!("PingActor started");
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        println!("PingActor stopped");
    }
}

impl Handler<Ping> for PingActor {
    fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn send_message() {
    let count = Arc::new(AtomicUsize::new(0));
    let actor = PingActor {
        count: count.clone(),
    };

    let sys = ActorSystem::new();
    let addr = sys.spawn(actor);

    for _ in 0..10 {
        addr.do_send(Ping);
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(count.load(Ordering::SeqCst), 10);
}

struct Calculator;

struct Add(u32, u32);
impl Message for Add {
    type Result = u32;
}

impl Actor for Calculator {}

impl Handler<Add> for Calculator {
    fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> u32 {
        msg.0 + msg.1
    }
}

#[tokio::test]
async fn request_response() {
    let sys = ActorSystem::new();
    let addr = sys.spawn(Calculator);

    let result = addr.send(Add(5, 7)).await.unwrap();
    assert_eq!(result, 12);

    let result = addr.send(Add(20, 22)).await.unwrap();
    assert_eq!(result, 42);
}

#[tokio::test]
async fn send_to_stopped_actor_fails() {
    let sys = ActorSystem::new();
    let addr = sys.spawn(Calculator);

    // Give actor time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Shutdown the system
    sys.shutdown();

    // Give actor time to fully stop and drop rx
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Drop sys so the task can fully clean up
    drop(sys);
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Actor is dead, should fail
    let result = addr.send(Add(1, 2)).await;
    assert!(result.is_err());
    assert_eq!(result.err().unwrap(), MailboxError::MailboxClosed);
}

struct Tick;
impl Message for Tick {
    type Result = ();
}

struct TickActor {
    count: Arc<AtomicU32>,
}

impl Actor for TickActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // Send Tick to self after 50ms
        ctx.run_later(Duration::from_millis(50), Tick);
    }
}

impl Handler<Tick> for TickActor {
    fn handle(&mut self, _msg: Tick, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn run_later_sends_delayed_message() {
    let count = Arc::new(AtomicU32::new(0));
    let actor = TickActor {
        count: count.clone(),
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Before delay
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(count.load(Ordering::SeqCst), 0);

    // After delay
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct Heartbeat;
impl Message for Heartbeat {
    type Result = ();
}

struct HeartbeatActor {
    count: Arc<AtomicU32>,
}

impl Actor for HeartbeatActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_millis(30), Heartbeat);
    }
}

impl Handler<Heartbeat> for HeartbeatActor {
    fn handle(&mut self, _msg: Heartbeat, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn run_interval_sends_periodic_messages() {
    let count = Arc::new(AtomicU32::new(0));
    let actor = HeartbeatActor {
        count: count.clone(),
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Wait for a few ticks (~100ms = ~3 ticks at 30ms interval)
    tokio::time::sleep(Duration::from_millis(100)).await;

    let ticks = count.load(Ordering::SeqCst);
    assert!(
        ticks >= 2 && ticks <= 4,
        "Expected 2-4 ticks, got {}",
        ticks
    );
}

// ======== Timer Cancellation Tests ========

struct CancelTickActor {
    count: Arc<AtomicU32>,
    handle: Option<TimerHandle>,
}

impl Actor for CancelTickActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // Schedule a tick that we'll cancel
        self.handle = Some(ctx.run_later(Duration::from_millis(50), Tick));
    }
}

impl Handler<Tick> for CancelTickActor {
    fn handle(&mut self, _msg: Tick, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn run_later_can_be_cancelled() {
    let count = Arc::new(AtomicU32::new(0));
    let actor = CancelTickActor {
        count: count.clone(),
        handle: None,
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Give actor time to start and set up the timer
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Timer not fired yet, count should be 0
    assert_eq!(count.load(Ordering::SeqCst), 0);

    // Wait past the timer delay
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Timer should have fired
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

struct CancelMsg;
impl Message for CancelMsg {
    type Result = ();
}

struct CancellableActor {
    count: Arc<AtomicU32>,
    handle: Option<TimerHandle>,
}

impl Actor for CancellableActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        self.handle = Some(ctx.run_later(Duration::from_millis(50), Tick));
    }
}

impl Handler<Tick> for CancellableActor {
    fn handle(&mut self, _msg: Tick, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Handler<CancelMsg> for CancellableActor {
    fn handle(&mut self, _msg: CancelMsg, _ctx: &mut Context<Self>) {
        if let Some(ref handle) = self.handle {
            handle.cancel();
        }
    }
}

#[tokio::test]
async fn cancelled_timer_does_not_fire() {
    let count = Arc::new(AtomicU32::new(0));
    let actor = CancellableActor {
        count: count.clone(),
        handle: None,
    };

    let sys = ActorSystem::new();
    let addr = sys.spawn(actor);

    tokio::time::sleep(Duration::from_millis(10)).await;

    addr.do_send(CancelMsg);

    // Wait past the timer delay
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Timer was cancelled, count should still be 0
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct IntervalTick;
impl Message for IntervalTick {
    type Result = ();
}

struct CancellableIntervalActor {
    count: Arc<AtomicU32>,
    handle: Option<TimerHandle>,
}

impl Actor for CancellableIntervalActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        self.handle = Some(ctx.run_interval(Duration::from_millis(20), IntervalTick));
    }
}

impl Handler<IntervalTick> for CancellableIntervalActor {
    fn handle(&mut self, _msg: IntervalTick, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Handler<CancelMsg> for CancellableIntervalActor {
    fn handle(&mut self, _msg: CancelMsg, _ctx: &mut Context<Self>) {
        if let Some(ref handle) = self.handle {
            handle.cancel();
        }
    }
}

#[tokio::test]
async fn cancelled_interval_stops_firing() {
    let count = Arc::new(AtomicU32::new(0));
    let actor = CancellableIntervalActor {
        count: count.clone(),
        handle: None,
    };

    let sys = ActorSystem::new();
    let addr = sys.spawn(actor);

    // Let a few ticks happen
    tokio::time::sleep(Duration::from_millis(50)).await;
    let count_before_cancel = count.load(Ordering::SeqCst);
    assert!(
        count_before_cancel >= 1,
        "Should have some ticks before cancel"
    );

    // Cancel the interval
    addr.do_send(CancelMsg);

    // Wait a bit for cancel to process
    tokio::time::sleep(Duration::from_millis(10)).await;
    let count_at_cancel = count.load(Ordering::SeqCst);

    // Wait more - no more ticks should happen
    tokio::time::sleep(Duration::from_millis(60)).await;
    let count_after = count.load(Ordering::SeqCst);

    assert_eq!(
        count_at_cancel, count_after,
        "Count should not increase after cancel"
    );
}
