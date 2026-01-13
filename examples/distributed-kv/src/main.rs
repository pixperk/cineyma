mod kv_store;

use cineyma::{
    remote::{ClusterClient, LocalNode, MessageRouter},
    ActorSystem,
};
use kv_store::KVStore;
use std::{env, sync::Arc, time::Duration};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/kv.rs"));
}

use proto::{DeleteRequest, GetRequest, SetRequest};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: distributed-kv <node-id> [<seed-addr>]");
        eprintln!("  node-id: unique identifier (e.g., node-1, node-2)");
        eprintln!("  seed-addr: optional address of existing cluster node (e.g., 127.0.0.1:7001)");
        std::process::exit(1);
    }

    let node_id = &args[1];
    let seed_addr = args.get(2);

    println!("starting node: {}", node_id);

    //parse node id to get port (node-1 -> 7001, node-2 -> 7002, etc)
    let port: u16 = if let Some(suffix) = node_id.strip_prefix("node-") {
        7000 + suffix.parse::<u16>().unwrap_or(1)
    } else {
        7001
    };

    let addr = format!("127.0.0.1:{}", port);

    //create cluster node
    let cluster = Arc::new(cineyma::remote::cluster::ClusterNode::new(
        node_id.clone(),
        addr.clone(),
    ));

    //create actor system and spawn kv store (only on node-1)
    let system = ActorSystem::new();
    let handler = if node_id == "node-1" {
        let store = system.spawn(KVStore::new());
        cluster
            .register_actor("kv-store".to_string(), "KVStore".to_string())
            .await;

        let local_node = LocalNode::new(node_id);
        Some(
            MessageRouter::new()
                .route::<GetRequest>(local_node.handler::<KVStore, GetRequest>(store.clone()))
                .route::<SetRequest>(local_node.handler::<KVStore, SetRequest>(store.clone()))
                .route::<DeleteRequest>(local_node.handler::<KVStore, DeleteRequest>(store.clone()))
                .build(),
        )
    } else {
        None
    };

    //start cluster server
    tokio::spawn(cluster.clone().start_server(port, handler));

    //join existing cluster if seed provided
    if let Some(seed) = seed_addr {
        println!("joining cluster via seed: {}", seed);
        let seed_node = cineyma::remote::cluster::Node {
            id: "temp-seed".to_string(),
            addr: seed.to_string(),
            status: cineyma::remote::cluster::NodeStatus::Up,
        };
        let _ = cluster.send_gossip_to(&seed_node).await;
    }

    //start periodic gossip
    let _gossip_handle = cluster
        .clone()
        .start_periodic_gossip(Duration::from_secs(2), Duration::from_secs(10));

    tokio::time::sleep(Duration::from_millis(100)).await;

    //interactive cli
    use std::io::{self, Write};

    println!("\nnode {} ready on {}", node_id, addr);
    println!("commands: get <key> | set <key> <value> | delete <key> | members | actors | quit\n");

    let client = ClusterClient::new(cluster.clone());

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let parts: Vec<&str> = input.trim().split_whitespace().collect();

        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "get" => {
                if parts.len() < 2 {
                    println!("usage: get <key>");
                    continue;
                }
                let key = parts[1];
                let remote = client.remote_addr::<KVStore>("kv-store");
                match remote
                    .call(GetRequest {
                        key: key.to_string(),
                    })
                    .await
                {
                    Ok(resp) => {
                        if let Some(value) = resp.value {
                            println!("{}", value);
                        } else {
                            println!("(nil)");
                        }
                    }
                    Err(e) => println!("error: {:?}", e),
                }
            }
            "set" => {
                if parts.len() < 3 {
                    println!("usage: set <key> <value>");
                    continue;
                }
                let key = parts[1];
                let value = parts[2..].join(" ");
                let remote = client.remote_addr::<KVStore>("kv-store");
                match remote
                    .call(SetRequest {
                        key: key.to_string(),
                        value: value.clone(),
                    })
                    .await
                {
                    Ok(_) => println!("OK"),
                    Err(e) => println!("error: {:?}", e),
                }
            }
            "delete" => {
                if parts.len() < 2 {
                    println!("usage: delete <key>");
                    continue;
                }
                let key = parts[1];
                let remote = client.remote_addr::<KVStore>("kv-store");
                match remote
                    .call(DeleteRequest {
                        key: key.to_string(),
                    })
                    .await
                {
                    Ok(resp) => {
                        if resp.deleted {
                            println!("deleted");
                        } else {
                            println!("not found");
                        }
                    }
                    Err(e) => println!("error: {:?}", e),
                }
            }
            "members" => {
                let members = cluster.get_members().await;
                println!("cluster members:");
                for member in members {
                    println!("  {} @ {} [{:?}]", member.id, member.addr, member.status);
                }
            }
            "actors" => {
                println!("registered actors:");
                if let Some((node_id, actor_type)) = cluster.lookup_actor("kv-store").await {
                    println!("  kv-store -> {} ({})", node_id, actor_type);
                }
            }
            "quit" | "exit" => {
                println!("shutting down...");
                break;
            }
            _ => {
                println!("unknown command: {}", parts[0]);
            }
        }
    }
}
