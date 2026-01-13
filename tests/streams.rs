use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use cineyma::{Actor, ActorSystem, Context, StreamHandler};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

// ======== Basic Stream Processing ========

struct StreamActor {
    count: Arc<AtomicUsize>,
    stream: Option<UnboundedReceiverStream<i32>>,
}

impl Actor for StreamActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Some(stream) = self.stream.take() {
            ctx.add_stream(stream);
        }
    }
}

impl StreamHandler<i32> for StreamActor {
    fn handle(&mut self, item: i32, _ctx: &mut Context<Self>) {
        println!("StreamActor received: {}", item);
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn actor_processes_stream_items() {
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::unbounded_channel::<i32>();
    let stream = UnboundedReceiverStream::new(rx);

    let actor = StreamActor {
        count: count.clone(),
        stream: Some(stream),
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Send items through stream
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(count.load(Ordering::SeqCst), 3);
}

// ======== Stream Finished Hook ========

struct FinishActor {
    count: Arc<AtomicUsize>,
    finished: Arc<AtomicBool>,
    stream: Option<UnboundedReceiverStream<i32>>,
}

impl Actor for FinishActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Some(stream) = self.stream.take() {
            ctx.add_stream(stream);
        }
    }
}

impl StreamHandler<i32> for FinishActor {
    fn handle(&mut self, _item: i32, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    fn finished(&mut self, _ctx: &mut Context<Self>) {
        println!("Stream finished!");
        self.finished.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn stream_finished_calls_hook() {
    let count = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::unbounded_channel::<i32>();
    let stream = UnboundedReceiverStream::new(rx);

    let actor = FinishActor {
        count: count.clone(),
        finished: finished.clone(),
        stream: Some(stream),
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Send some items
    tx.send(1).unwrap();
    tx.send(2).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Drop sender to end stream
    drop(tx);

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(
        finished.load(Ordering::SeqCst),
        "finished() should be called"
    );
}

// ======== Multiple Streams ========

struct MultiStreamActor {
    int_count: Arc<AtomicUsize>,
    str_count: Arc<AtomicUsize>,
    int_stream: Option<UnboundedReceiverStream<i32>>,
    str_stream: Option<UnboundedReceiverStream<String>>,
}

impl Actor for MultiStreamActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Some(stream) = self.int_stream.take() {
            ctx.add_stream(stream);
        }
        if let Some(stream) = self.str_stream.take() {
            ctx.add_stream(stream);
        }
    }
}

impl StreamHandler<i32> for MultiStreamActor {
    fn handle(&mut self, item: i32, _ctx: &mut Context<Self>) {
        println!("Got int: {}", item);
        self.int_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl StreamHandler<String> for MultiStreamActor {
    fn handle(&mut self, item: String, _ctx: &mut Context<Self>) {
        println!("Got string: {}", item);
        self.str_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn actor_handles_multiple_streams() {
    let int_count = Arc::new(AtomicUsize::new(0));
    let str_count = Arc::new(AtomicUsize::new(0));

    let (int_tx, int_rx) = mpsc::unbounded_channel::<i32>();
    let (str_tx, str_rx) = mpsc::unbounded_channel::<String>();

    let actor = MultiStreamActor {
        int_count: int_count.clone(),
        str_count: str_count.clone(),
        int_stream: Some(UnboundedReceiverStream::new(int_rx)),
        str_stream: Some(UnboundedReceiverStream::new(str_rx)),
    };

    let sys = ActorSystem::new();
    let _addr = sys.spawn(actor);

    // Send items on both streams
    int_tx.send(1).unwrap();
    int_tx.send(2).unwrap();
    str_tx.send("hello".to_string()).unwrap();
    str_tx.send("world".to_string()).unwrap();
    str_tx.send("!".to_string()).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(int_count.load(Ordering::SeqCst), 2);
    assert_eq!(str_count.load(Ordering::SeqCst), 3);
}
