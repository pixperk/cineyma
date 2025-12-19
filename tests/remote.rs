use std::sync::Arc;

use cinema::{
    remote::{
        deserialize_payload, proto::Envelope, register_message, Connection, EnvelopeHandler,
        LocalNode, RemoteAddr, RemoteClient, RemoteMessage, RemoteServer, TcpConnection,
        TcpTransport, Transport,
    },
    Actor, ActorSystem, Context, Handler, Message,
};
use prost::Message as ProstMessage;
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, ProstMessage)]
struct Ping {
    #[prost(string, tag = "1")]
    message: String,
}

impl Message for Ping {
    type Result = ();
}

impl RemoteMessage for Ping {}

#[test]
fn envelope_roundtrip() {
    let ping = Ping {
        message: "Hello, World!".to_string(),
    };

    let envelope = Envelope::from_message(&ping, 42, "node", "actor");

    assert!(envelope.message_type.contains("Ping")); // auto-derived type name
    assert_eq!(envelope.correlation_id, 42);

    let serialized = envelope.to_bytes();

    let decoded = Envelope::from_bytes(&serialized).unwrap();
    assert!(decoded.message_type.contains("Ping"));
    assert_eq!(decoded.correlation_id, 42);
}

#[test]
fn registry_deserialize() {
    register_message::<Ping>();

    let ping = Ping {
        message: "Hello, Registry!".to_string(),
    };
    let envelope = Envelope::from_message(&ping, 1, "node", "actor");

    let deserialized = deserialize_payload(&envelope.message_type, &envelope.payload).unwrap();
    let downcasted = deserialized.downcast_ref::<Ping>().unwrap();

    assert_eq!(downcasted.message, "Hello, Registry!");
}

#[tokio::test]
async fn tcp_send_recv_envelope() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    //spawn server task
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut conn = TcpConnection::new(stream);

        //receive envelope
        let envelope = conn.recv().await.unwrap();
        assert!(envelope.message_type.contains("Ping"));
        println!(
            "Server received: {}",
            String::from_utf8_lossy(&envelope.payload)
        );

        envelope
    });

    //client task
    let transport = TcpTransport;
    let mut client = transport.connect(&addr.to_string()).await.unwrap();

    let ping = Ping {
        message: "Hello TCP!".to_string(),
    };
    let envelope = Envelope::from_message(&ping, 123, "node-a", "actor-1");

    client.send(envelope).await.unwrap();
    client.close().await.unwrap();

    let received_envelope = server.await.unwrap();
    assert!(received_envelope.message_type.contains("Ping"));
    assert_eq!(received_envelope.correlation_id, 123);
    assert_eq!(received_envelope.sender_node, "node-a");
    assert_eq!(received_envelope.target_actor, "actor-1");

    register_message::<Ping>();

    let deserialized =
        deserialize_payload(&received_envelope.message_type, &received_envelope.payload).unwrap();
    let downcasted = deserialized.downcast_ref::<Ping>().unwrap();

    assert_eq!(downcasted.message, "Hello TCP!");
}

#[tokio::test]
async fn remote_client_send_recv() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut conn = TcpConnection::new(stream);

        //recv request
        let request = conn.recv().await.unwrap();
        assert_eq!(request.message_type, "test::Ping");
        println!(
            "Server received request from correlation id {}",
            request.correlation_id
        );

        //send response with same id
        let resp = Envelope {
            message_type: "test::Pong".to_string(),
            payload: b"Pong response".to_vec(),
            correlation_id: request.correlation_id,
            sender_node: "node-server".to_string(),
            target_actor: request.sender_node.clone(),
            is_response: true, //mark as response
        };

        conn.send(resp).await.unwrap();
    });

    //client task
    let stream = TcpStream::connect(addr).await.unwrap();
    let tcp_conn = TcpConnection::new(stream);
    let client = RemoteClient::new(tcp_conn);

    let request = Envelope {
        message_type: "test::Ping".to_string(),
        payload: b"ping data".to_vec(),
        correlation_id: 42,
        sender_node: "client".to_string(),
        target_actor: "some_actor".to_string(),
        is_response: false,
    };

    let response = client.send(request).await.unwrap();

    assert_eq!(response.correlation_id, 42);
    assert!(response.is_response);
    assert_eq!(response.message_type, "test::Pong");
    println!("Client got response: {:?}", response.message_type);

    server.await.unwrap();
}

#[tokio::test]
async fn remote_addr_to_server() {
    let handler: EnvelopeHandler = Arc::new(|envelope: Envelope| {
        Box::pin(async move {
            println!("Server handling: {}", envelope.message_type);

            Some(Envelope {
                message_type: "test::Pong".to_string(),
                payload: b"pong".to_vec(),
                correlation_id: envelope.correlation_id,
                sender_node: "server".to_string(),
                target_actor: envelope.sender_node.clone(),
                is_response: true,
            })
        })
    });

    let server = RemoteServer::bind("127.0.0.1:0", handler).await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let transport = TcpTransport;
    let conn = transport.connect(&addr.to_string()).await.unwrap();
    let client = RemoteClient::new(conn);

    let remote: RemoteAddr<()> = RemoteAddr::new("client", "server-node", "echo-actor", client);

    // Send via RemoteAddr
    register_message::<Ping>();
    let response = remote
        .send(Ping {
            message: "hello".to_string(),
        })
        .await
        .unwrap();

    assert!(response.is_response);
    assert!(response.correlation_id > 0); // correlation ID is global, just check it exists
    println!("Got response: {:?}", response.message_type);
}

#[tokio::test]
async fn remote_addr_to_actor() {
    // Define a simple Counter actor
    struct Counter {
        count: i32,
    }
    impl Actor for Counter {}

    // Increment message (must be RemoteMessage)
    #[derive(Clone, prost::Message)]
    struct Increment {
        #[prost(int32, tag = "1")]
        amount: i32,
    }
    impl Message for Increment {
        type Result = i32;
    }
    impl RemoteMessage for Increment {}
    impl Handler<Increment> for Counter {
        fn handle(&mut self, msg: Increment, _ctx: &mut Context<Self>) -> i32 {
            self.count += msg.amount;
            self.count
        }
    }

    // Start actor system
    let system = ActorSystem::new();
    let counter_addr = system.spawn(Counter { count: 0 });

    // Create handler that dispatches to Counter actor
    let handler: EnvelopeHandler = {
        let addr = counter_addr.clone();
        Arc::new(move |envelope| {
            let addr = addr.clone();
            Box::pin(async move {
                // Decode the Increment message
                let msg = Increment::decode(envelope.payload.as_slice()).ok()?;

                // Send to actor
                let result = addr.send(msg).await.ok()?;

                // Build response (just put result in payload as bytes)
                Some(Envelope {
                    message_type: "i32".to_string(),
                    payload: result.to_be_bytes().to_vec(),
                    correlation_id: envelope.correlation_id,
                    sender_node: "server".to_string(),
                    target_actor: envelope.sender_node.clone(),
                    is_response: true,
                })
            })
        })
    };

    // Start server
    let server = RemoteServer::bind("127.0.0.1:0", handler).await.unwrap();
    let server_addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Client connects
    let transport = TcpTransport;
    let conn = transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RemoteClient::new(conn);
    let remote: RemoteAddr<Counter> = RemoteAddr::new("client", "server", "counter", client);

    // Send Increment via RemoteAddr
    register_message::<Increment>();
    let response = remote.send(Increment { amount: 5 }).await.unwrap();

    assert!(response.is_response);
    let result = i32::from_be_bytes(response.payload.try_into().unwrap());
    assert_eq!(result, 5);
    println!("Remote actor returned: {}", result);
}

/// Test using make_handler helper - much cleaner than manual handler
#[tokio::test]
async fn make_handler_simplifies_setup() {
    // Actor
    struct Calculator {
        value: i32,
    }
    impl Actor for Calculator {}

    // Request message (protobuf)
    #[derive(Clone, prost::Message)]
    struct Add {
        #[prost(int32, tag = "1")]
        n: i32,
    }
    impl Message for Add {
        type Result = AddResult; // Result must also be RemoteMessage
    }
    impl RemoteMessage for Add {}

    // Response type (protobuf wrapper for the result)
    #[derive(Clone, prost::Message)]
    struct AddResult {
        #[prost(int32, tag = "1")]
        value: i32,
    }
    impl Message for AddResult {
        type Result = ();
    }
    impl RemoteMessage for AddResult {}

    impl Handler<Add> for Calculator {
        fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> AddResult {
            self.value += msg.n;
            AddResult { value: self.value }
        }
    }

    // Setup
    let system = ActorSystem::new();
    let calc_addr = system.spawn(Calculator { value: 10 });

    // Configure node identity once, use for all handlers
    let node = LocalNode::new("calc-node");
    let handler = node.handler::<Calculator, Add>(calc_addr.clone());

    // Start server
    let server = RemoteServer::bind("127.0.0.1:0", handler).await.unwrap();
    let server_addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Client
    let transport = TcpTransport;
    let conn = transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RemoteClient::new(conn);
    // Use LocalNode to create remote address (provides our identity automatically)
    let client_node = LocalNode::new("client");
    let remote: RemoteAddr<Calculator> = client_node.remote_addr("calc-node", "calculator", client);

    let response = remote.send(Add { n: 5 }).await.unwrap();

    // Decode protobuf result
    let result = AddResult::decode(response.payload.as_slice()).unwrap();
    assert_eq!(result.value, 15); // 10 + 5
    println!("Calculator returned: {}", result.value);
}

/// Test auto-derived client identity from TCP socket address
#[tokio::test]
async fn auto_derived_client_identity() {
    // Same actor setup as make_handler_simplifies_setup
    struct Calculator {
        value: i32,
    }
    impl Actor for Calculator {}

    #[derive(Clone, prost::Message)]
    struct Add {
        #[prost(int32, tag = "1")]
        n: i32,
    }
    impl Message for Add {
        type Result = AddResult;
    }
    impl RemoteMessage for Add {}

    #[derive(Clone, prost::Message)]
    struct AddResult {
        #[prost(int32, tag = "1")]
        value: i32,
    }
    impl Message for AddResult {
        type Result = ();
    }
    impl RemoteMessage for AddResult {}

    impl Handler<Add> for Calculator {
        fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> AddResult {
            self.value += msg.n;
            AddResult { value: self.value }
        }
    }

    let system = ActorSystem::new();
    let calc_addr = system.spawn(Calculator { value: 100 });

    let node = LocalNode::new("calc-server");
    let handler = node.handler::<Calculator, Add>(calc_addr.clone());

    let server = RemoteServer::bind("127.0.0.1:0", handler).await.unwrap();
    let server_addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Client - use auto-derived identity (no LocalNode needed!)
    let transport = TcpTransport;
    let conn = transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RemoteClient::new(conn);

    // Check that local_addr is derived from socket
    println!("Client local address: {}", client.local_addr());
    assert!(client.local_addr().contains("127.0.0.1"));

    // Create remote address using the simpler API
    let remote: RemoteAddr<Calculator> = client.remote_addr("calc-server", "calculator");

    let response = remote.send(Add { n: 7 }).await.unwrap();
    let result = AddResult::decode(response.payload.as_slice()).unwrap();
    assert_eq!(result.value, 107); // 100 + 7
    println!("Auto-identity client got: {}", result.value);
}

/// Test: Two servers with SAME node name - what happens?
#[tokio::test]
async fn two_servers_same_name() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CALC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static HELLO_CALLS: AtomicUsize = AtomicUsize::new(0);

    // Calculator Actor
    struct Calculator { value: i32 }
    impl Actor for Calculator {}

    #[derive(Clone, prost::Message)]
    struct Add { #[prost(int32, tag = "1")] n: i32 }
    impl Message for Add { type Result = AddResult; }
    impl RemoteMessage for Add {}

    #[derive(Clone, prost::Message)]
    struct AddResult { #[prost(int32, tag = "1")] value: i32 }
    impl Message for AddResult { type Result = (); }
    impl RemoteMessage for AddResult {}

    impl Handler<Add> for Calculator {
        fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> AddResult {
            CALC_CALLS.fetch_add(1, Ordering::SeqCst);
            println!("Calculator handling Add!");
            self.value += msg.n;
            AddResult { value: self.value }
        }
    }

    // HelloPrinter Actor
    struct HelloPrinter;
    impl Actor for HelloPrinter {}

    impl Handler<Add> for HelloPrinter {
        fn handle(&mut self, _msg: Add, _ctx: &mut Context<Self>) -> AddResult {
            HELLO_CALLS.fetch_add(1, Ordering::SeqCst);
            println!("Hello from calc! (not calculating)");
            AddResult { value: 999 }
        }
    }

    let system = ActorSystem::new();

    // Server 1: Calculator - named "calc-server"
    let calc_addr = system.spawn(Calculator { value: 100 });
    let node1 = LocalNode::new("calc-server");
    let server1 = RemoteServer::bind("127.0.0.1:0", node1.handler::<Calculator, Add>(calc_addr)).await.unwrap();
    let server1_addr = server1.local_addr().unwrap();
    tokio::spawn(server1.run());

    // Server 2: HelloPrinter - ALSO named "calc-server"!
    let hello_addr = system.spawn(HelloPrinter);
    let node2 = LocalNode::new("calc-server");
    let server2 = RemoteServer::bind("127.0.0.1:0", node2.handler::<HelloPrinter, Add>(hello_addr)).await.unwrap();
    let server2_addr = server2.local_addr().unwrap();
    tokio::spawn(server2.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    println!("Server 1 (Calculator) at: {}", server1_addr);
    println!("Server 2 (HelloPrinter) at: {}", server2_addr);

    let transport = TcpTransport;

    // Client 1 connects to Calculator
    let conn1 = transport.connect(&server1_addr.to_string()).await.unwrap();
    let client1 = RemoteClient::new(conn1);
    let remote1: RemoteAddr<Calculator> = client1.remote_addr("calc-server", "calculator");

    // Client 2 connects to HelloPrinter
    let conn2 = transport.connect(&server2_addr.to_string()).await.unwrap();
    let client2 = RemoteClient::new(conn2);
    let remote2: RemoteAddr<HelloPrinter> = client2.remote_addr("calc-server", "calculator");

    let result1 = AddResult::decode(remote1.send(Add { n: 5 }).await.unwrap().payload.as_slice()).unwrap();
    let result2 = AddResult::decode(remote2.send(Add { n: 5 }).await.unwrap().payload.as_slice()).unwrap();

    println!("Response from server1: {}", result1.value);
    println!("Response from server2: {}", result2.value);

    assert_eq!(result1.value, 105); // Calculator: 100 + 5
    assert_eq!(result2.value, 999); // HelloPrinter: dummy

    println!("\n=== CONCLUSION ===");
    println!("Node name doesn't matter for routing!");
    println!("TCP connection determines which server handles the message.");
}
