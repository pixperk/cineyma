use cinema::remote::cluster::{ClusterNode, Node, NodeStatus};

#[tokio::test]
async fn gossip_merges_membership() {
    let node_a = ClusterNode::new("node-a".to_string(), "127.0.0.1:8001".to_string());

    // Node B knows about itself and Node C
    let node_b = ClusterNode::new("node-b".to_string(), "127.0.0.1:8002".to_string());
    node_b
        .add_member(Node {
            id: "node-c".to_string(),
            addr: "127.0.0.1:8003".to_string(),
            status: NodeStatus::Up,
        })
        .await;

    // Node A sends gossip to Node B
    let gossip_from_a = node_a.create_gossip_message().await;
    node_b.merge_gossip(gossip_from_a).await;

    // Node B now knows about A
    let members_b = node_b.get_members().await;
    assert_eq!(members_b.len(), 3); // B, C, and now A
    assert!(members_b.iter().any(|n| n.id == "node-a"));

    // Node B sends gossip back to Node A
    let gossip_from_b = node_b.create_gossip_message().await;
    node_a.merge_gossip(gossip_from_b).await;

    // Node A now knows about B and C
    let members_a = node_a.get_members().await;
    assert_eq!(members_a.len(), 3); // A, B, and C
    assert!(members_a.iter().any(|n| n.id == "node-b"));
    assert!(members_a.iter().any(|n| n.id == "node-c"));

    println!(
        "Node A members: {:?}",
        members_a.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn seven_nodes_gossip_converge() {
    use std::sync::Arc;

    // Create 7 nodes
    let nodes: Vec<Arc<ClusterNode>> = (1..=7)
        .map(|i| {
            Arc::new(ClusterNode::new(
                format!("node-{}", i),
                format!("127.0.0.1:{}", 9100 + i),
            ))
        })
        .collect();

    // Start servers
    for (i, node) in nodes.iter().enumerate() {
        let port = 9101 + i as u16;
        tokio::spawn(node.clone().start_gossip_server(port));
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Chain: 1->2->3->4->5->6->7
    for i in 0..6 {
        let peer = Node {
            id: format!("node-{}", i + 2),
            addr: format!("127.0.0.1:{}", 9101 + (i + 1)),
            status: NodeStatus::Up,
        };
        nodes[i].send_gossip_to(&peer).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Reverse gossip to spread info back
    for i in (1..7).rev() {
        let peer = Node {
            id: format!("node-{}", i),
            addr: format!("127.0.0.1:{}", 9100 + i),
            status: NodeStatus::Up,
        };
        nodes[i].send_gossip_to(&peer).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // All nodes should know all 7
    for (i, node) in nodes.iter().enumerate() {
        let members = node.get_members().await;
        println!(
            "Node {} knows: {:?}",
            i + 1,
            members.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert_eq!(members.len(), 7, "Node {} should know 7 nodes", i + 1);
    }
}

#[tokio::test]
async fn periodic_gossip_spreads_membership() {
    use std::sync::Arc;
    use std::time::Duration;

    // Create 3 nodes (addr = gossip server port)
    let node1 = Arc::new(ClusterNode::new(
        "node-1".to_string(),
        "127.0.0.1:9211".to_string(),
    ));
    let node2 = Arc::new(ClusterNode::new(
        "node-2".to_string(),
        "127.0.0.1:9212".to_string(),
    ));
    let node3 = Arc::new(ClusterNode::new(
        "node-3".to_string(),
        "127.0.0.1:9213".to_string(),
    ));

    // Start gossip servers on same ports as node addresses
    tokio::spawn(node1.clone().start_gossip_server(9211));
    tokio::spawn(node2.clone().start_gossip_server(9212));
    tokio::spawn(node3.clone().start_gossip_server(9213));
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Node1 knows about Node2
    node1
        .add_member(Node {
            id: "node-2".to_string(),
            addr: "127.0.0.1:9212".to_string(),
            status: NodeStatus::Up,
        })
        .await;

    // Node2 knows about Node3
    node2
        .add_member(Node {
            id: "node-3".to_string(),
            addr: "127.0.0.1:9213".to_string(),
            status: NodeStatus::Up,
        })
        .await;

    // Start periodic gossip (every 100ms, suspect after 10s - not testing failure here)
    let _handle1 = node1
        .clone()
        .start_periodic_gossip(Duration::from_millis(100), Duration::from_secs(10));
    let _handle2 = node2
        .clone()
        .start_periodic_gossip(Duration::from_millis(100), Duration::from_secs(10));
    let _handle3 = node3
        .clone()
        .start_periodic_gossip(Duration::from_millis(100), Duration::from_secs(10));

    // Wait for gossip to spread (should take 2-3 rounds)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // All nodes should eventually know all 3 nodes
    let members1 = node1.get_members().await;
    let members2 = node2.get_members().await;
    let members3 = node3.get_members().await;

    println!(
        "Node 1 knows: {:?}",
        members1.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    println!(
        "Node 2 knows: {:?}",
        members2.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    println!(
        "Node 3 knows: {:?}",
        members3.iter().map(|n| &n.id).collect::<Vec<_>>()
    );

    assert_eq!(members1.len(), 3, "Node 1 should know 3 nodes");
    assert_eq!(members2.len(), 3, "Node 2 should know 3 nodes");
    assert_eq!(members3.len(), 3, "Node 3 should know 3 nodes");
}

#[tokio::test]
async fn failure_detector_marks_dead_nodes() {
    use std::sync::Arc;
    use std::time::Duration;

    // Create node1 (node2 is just for the test, doesn't need to run)
    let node1 = Arc::new(ClusterNode::new(
        "node-1".to_string(),
        "127.0.0.1:9301".to_string(),
    ));

    // Node1 knows about Node2
    node1
        .add_member(Node {
            id: "node-2".to_string(),
            addr: "127.0.0.1:9302".to_string(),
            status: NodeStatus::Up,
        })
        .await;

    // Start periodic gossip with failure detection (check every 100ms, suspect after 200ms)
    let _handle = node1.clone().start_periodic_gossip(
        Duration::from_millis(100),
        Duration::from_millis(200),
    );

    // Wait for suspect timeout
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Node2 should be marked as SUSPECT
    let members = node1.get_members().await;
    let node2_status = members
        .iter()
        .find(|n| n.id == "node-2")
        .map(|n| &n.status);
    assert_eq!(node2_status, Some(&NodeStatus::Suspect));
    println!("Node 2 marked as SUSPECT");

    // Wait for down timeout (2x suspect = 400ms total)
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Node2 should be marked as DOWN
    let members = node1.get_members().await;
    let node2_status = members
        .iter()
        .find(|n| n.id == "node-2")
        .map(|n| &n.status);
    assert_eq!(node2_status, Some(&NodeStatus::Down));
    println!("Node 2 marked as DOWN");
}
