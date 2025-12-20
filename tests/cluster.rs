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
