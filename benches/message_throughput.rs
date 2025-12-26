use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use cinema::ActorSystem;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

mod common;
use common::{Count, CounterActor};

fn bench_do_send_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_throughput");

    // single actor, varying message count
    for msg_count in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("do_send_single_actor", msg_count),
            msg_count,
            |b, &msg_count| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async move {
                        let count = Arc::new(AtomicUsize::new(0));
                        let sys = ActorSystem::new();
                        let addr = sys.spawn(CounterActor {
                            count: count.clone(),
                        });

                        // send messages
                        for _ in 0..msg_count {
                            addr.do_send(Count);
                        }

                        // wait for processing
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        black_box(count.load(Ordering::Relaxed));
                    });
            },
        );
    }

    // multiple actors, parallel throughput
    group.bench_function("do_send_parallel_100actors_1000msgs", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let sys = ActorSystem::new();
                let mut actors = Vec::new();

                for _ in 0..100 {
                    let count = Arc::new(AtomicUsize::new(0));
                    actors.push((
                        sys.spawn(CounterActor {
                            count: count.clone(),
                        }),
                        count,
                    ));
                }

                // send to all actors
                for (addr, _) in &actors {
                    for _ in 0..1000 {
                        addr.do_send(Count);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                black_box(&actors);
            });
    });

    group.finish();
}

criterion_group!(benches, bench_do_send_throughput);
criterion_main!(benches);
