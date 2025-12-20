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
            addr: format!("127.0.0.1:{}", 9102 + i),
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
