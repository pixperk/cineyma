use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use cinema::ActorSystem;

mod common;
use common::NoOpActor;

fn bench_spawn_noop(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_spawn");

    group.bench_function("spawn_single_noop", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let sys = ActorSystem::new();
                let addr = sys.spawn(NoOpActor);
                black_box(addr);
            });
    });

    // batch spawn to measure scaling
    for count in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("spawn_batch", count), count, |b, &count| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async move {
                    let sys = ActorSystem::new();
                    let mut addrs = Vec::with_capacity(count);
                    for _ in 0..count {
                        addrs.push(sys.spawn(NoOpActor));
                    }
                    black_box(addrs);
                });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_spawn_noop);
criterion_main!(benches);
