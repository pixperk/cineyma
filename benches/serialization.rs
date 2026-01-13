use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use prost::Message as ProstMessage;

mod common;
use common::{create_payload, BenchRequest};

fn bench_prost_encode_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    // vary payload size: 64B, 1KB, 16KB, 256KB
    for size in [64, 1024, 16384, 262144].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        let payload = create_payload(*size);
        let msg = BenchRequest {
            request_id: 12345,
            payload: payload.clone(),
        };

        // encode benchmark
        group.bench_with_input(BenchmarkId::new("prost_encode", size), &msg, |b, msg| {
            b.iter(|| {
                let mut buf = Vec::new();
                msg.encode(&mut buf).unwrap();
                black_box(buf);
            });
        });

        // decode benchmark
        let encoded = {
            let mut buf = Vec::new();
            msg.encode(&mut buf).unwrap();
            buf
        };

        group.bench_with_input(
            BenchmarkId::new("prost_decode", size),
            &encoded,
            |b, encoded| {
                b.iter(|| {
                    let decoded = BenchRequest::decode(encoded.as_slice()).unwrap();
                    black_box(decoded);
                });
            },
        );

        // round-trip (encode + decode)
        group.bench_with_input(BenchmarkId::new("roundtrip", size), &msg, |b, msg| {
            b.iter(|| {
                let mut buf = Vec::new();
                msg.encode(&mut buf).unwrap();
                let decoded = BenchRequest::decode(buf.as_slice()).unwrap();
                black_box(decoded);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_prost_encode_decode);
criterion_main!(benches);
