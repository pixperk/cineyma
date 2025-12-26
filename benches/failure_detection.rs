use cinema::remote::cluster::{ClusterNode, Node, NodeStatus};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::sync::Arc;
use std::time::Duration;

mod common;
use common::get_bench_port;

fn bench_failure_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("failure_detection");
    group.measurement_time(Duration::from_secs(10)); // longer measurement for periodic tests

    // detection latency benchmark
    group.bench_function("detect_suspect_3nodes", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let node1 = Arc::new(ClusterNode::new(
                    "node-1".to_string(),
                    format!("127.0.0.1:{}", get_bench_port(300)),
                ));
                let node2 = Arc::new(ClusterNode::new(
                    "node-2".to_string(),
                    format!("127.0.0.1:{}", get_bench_port(301)),
                ));
                let node3 = Arc::new(ClusterNode::new(
                    "node-3".to_string(),
                    format!("127.0.0.1:{}", get_bench_port(302)),
                ));

                // start servers
                tokio::spawn(node1.clone().start_gossip_server(get_bench_port(300)));
                tokio::spawn(node2.clone().start_gossip_server(get_bench_port(301)));
                tokio::spawn(node3.clone().start_gossip_server(get_bench_port(302)));
                tokio::time::sleep(Duration::from_millis(20)).await;

                // connect nodes
                node1
                    .add_member(Node {
                        id: "node-2".to_string(),
                        addr: format!("127.0.0.1:{}", get_bench_port(301)),
                        status: NodeStatus::Up,
                    })
                    .await;
                node1
                    .add_member(Node {
                        id: "node-3".to_string(),
                        addr: format!("127.0.0.1:{}", get_bench_port(302)),
                        status: NodeStatus::Up,
                    })
                    .await;

                // start periodic gossip with tight timeouts
                let suspect_timeout = Duration::from_millis(100);
                let _handle = node1.clone().start_periodic_gossip(
                    Duration::from_millis(50),
                    suspect_timeout,
                );

                // measure time to detect node-3 as suspect (it won't send gossip back)
                let start = std::time::Instant::now();
                loop {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let members = node1.get_members().await;
                    if let Some(node) = members.iter().find(|n| n.id == "node-3") {
                        if node.status == NodeStatus::Suspect {
                            break;
                        }
                    }

                    // timeout after 1 second
                    if start.elapsed() > Duration::from_secs(1) {
                        break;
                    }
                }

                let detection_time = start.elapsed();
                black_box(detection_time);
            });
    });

    // heartbeat update overhead
    for node_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("heartbeat_check_overhead", node_count),
            node_count,
            |b, &node_count| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async move {
                    let node = ClusterNode::new(
                        "bench-node".to_string(),
                        format!("127.0.0.1:{}", get_bench_port(400)),
                    );

                    // add many members
                    for i in 0..node_count {
                        node.add_member(Node {
                            id: format!("node-{}", i),
                            addr: format!("127.0.0.1:{}", get_bench_port(400 + i as u16)),
                            status: NodeStatus::Up,
                        })
                        .await;
                    }

                    // simulate single heartbeat check cycle (what periodic gossip does)
                    let start = std::time::Instant::now();
                    let members = node.get_members().await;
                    black_box(members);
                    let check_time = start.elapsed();

                    black_box(check_time);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_failure_detection);
criterion_main!(benches);
