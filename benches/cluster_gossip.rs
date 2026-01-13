use cineyma::remote::cluster::{ClusterNode, Node, NodeStatus};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::sync::Arc;

mod common;
use common::get_bench_port;

fn bench_gossip_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("cluster_gossip");

    // gossip message creation (scales with members + actors)
    for member_count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_gossip", member_count),
            member_count,
            |b, &member_count| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async move {
                    let node = ClusterNode::new(
                        "bench-node".to_string(),
                        format!("127.0.0.1:{}", get_bench_port(0)),
                    );

                    // add members
                    for i in 0..member_count {
                        node.add_member(Node {
                            id: format!("node-{}", i),
                            addr: format!("127.0.0.1:{}", get_bench_port(i as u16)),
                            status: NodeStatus::Up,
                        })
                        .await;
                    }

                    // register actors
                    for i in 0..member_count {
                        node.register_actor(format!("actor-{}", i), "BenchActor".to_string())
                            .await;
                    }

                    let gossip = node.create_gossip_message().await;
                    black_box(gossip);
                });
            },
        );
    }

    // gossip merge (read lock contention)
    group.bench_function("merge_gossip_50nodes", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(&rt).iter(|| async {
            let node_a = ClusterNode::new(
                "node-a".to_string(),
                format!("127.0.0.1:{}", get_bench_port(100)),
            );
            let node_b = ClusterNode::new(
                "node-b".to_string(),
                format!("127.0.0.1:{}", get_bench_port(101)),
            );

            // node B has 50 members
            for i in 0..50 {
                node_b
                    .add_member(Node {
                        id: format!("node-{}", i),
                        addr: format!("127.0.0.1:{}", get_bench_port(100 + i as u16)),
                        status: NodeStatus::Up,
                    })
                    .await;
            }

            let gossip = node_b.create_gossip_message().await;
            node_a.merge_gossip(gossip, "node-b").await;
            black_box(&node_a);
        });
    });

    // convergence time benchmark (realistic scenario)
    group.bench_function("convergence_7nodes_chain", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let nodes: Vec<Arc<ClusterNode>> = (1..=7)
                    .map(|i| {
                        Arc::new(ClusterNode::new(
                            format!("node-{}", i),
                            format!("127.0.0.1:{}", get_bench_port(200 + i as u16)),
                        ))
                    })
                    .collect();

                // start servers
                for (i, node) in nodes.iter().enumerate() {
                    let port = get_bench_port(201 + i as u16);
                    tokio::spawn(node.clone().start_gossip_server(port));
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                // chain gossip
                let start = std::time::Instant::now();
                for i in 0..6 {
                    let peer = Node {
                        id: format!("node-{}", i + 2),
                        addr: format!("127.0.0.1:{}", get_bench_port(201 + (i + 1) as u16)),
                        status: NodeStatus::Up,
                    };
                    nodes[i].send_gossip_to(&peer).await.unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }

                // reverse gossip
                for i in (1..7).rev() {
                    let peer = Node {
                        id: format!("node-{}", i),
                        addr: format!("127.0.0.1:{}", get_bench_port(200 + i as u16)),
                        status: NodeStatus::Up,
                    };
                    nodes[i].send_gossip_to(&peer).await.unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }

                // wait for convergence
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let elapsed = start.elapsed();
                black_box((nodes, elapsed));
            });
    });

    group.finish();
}

criterion_group!(benches, bench_gossip_operations);
criterion_main!(benches);
