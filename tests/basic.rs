use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use cinema::{Actor, ActorSystem, Context, Handler, Message};

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
