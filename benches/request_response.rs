use cinema::ActorSystem;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

mod common;
use common::{Add, AsyncActor, AsyncCompute, Calculator};

fn bench_sync_request_response(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_response");

    // sync handler latency
    group.bench_function("sync_local_add", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // spawn actor inside runtime context
        let (_sys, addr) = rt.block_on(async {
            let sys = ActorSystem::new();
            let addr = sys.spawn(Calculator);
            (sys, addr)
        });

        b.to_async(&rt).iter(|| async {
            black_box(addr.send(Add(42, 58)).await.unwrap());
        });
    });

    // async handler latency
    group.bench_function("async_local_compute", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // spawn actor inside runtime context
        let (_sys, addr) = rt.block_on(async {
            let sys = ActorSystem::new();
            let addr = sys.spawn(AsyncActor);
            (sys, addr)
        });

        b.to_async(&rt).iter(|| async {
            black_box(addr.send_async(AsyncCompute(100)).await.unwrap());
        });
    });

    // batched requests (pipelining effect)
    for batch_size in [10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("sync_batched", batch_size),
            batch_size,
            |b, &batch_size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                // spawn actor inside runtime context
                let (_sys, addr) = rt.block_on(async {
                    let sys = ActorSystem::new();
                    let addr = sys.spawn(Calculator);
                    (sys, addr)
                });

                b.to_async(&rt).iter(|| {
                    let addr = addr.clone();
                    async move {
                        let mut futures = Vec::new();
                        for i in 0..batch_size {
                            futures.push(addr.send(Add(i, i + 1)));
                        }
                        let results = futures::future::join_all(futures).await;
                        black_box(results);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_sync_request_response);
criterion_main!(benches);
