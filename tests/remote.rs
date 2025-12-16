use std::sync::Arc;

use cinema::{
    remote::{
        deserialize_payload, proto::Envelope, register_message, Connection, RemoteAddr,
        RemoteClient, RemoteMessage, RemoteServer, TcpConnection, TcpTransport, Transport,
    },
    Message,
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

impl RemoteMessage for Ping {
    fn type_id() -> &'static str {
        "test::Ping"
    }
}

#[test]
fn envelope_roundtrip() {
    let ping = Ping {
        message: "Hello, World!".to_string(),
    };

    let envelope = Envelope::from_message(&ping, 42, "node", "actor");

    assert_eq!(envelope.message_type, "test::Ping");
    assert_eq!(envelope.correlation_id, 42);

    let serialized = envelope.to_bytes();

    let decoded = Envelope::from_bytes(&serialized).unwrap();
    assert_eq!(decoded.message_type, "test::Ping");
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
        assert_eq!(envelope.message_type, "test::Ping");
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
    assert_eq!(received_envelope.message_type, "test::Ping");
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
    let handler = Arc::new(|envelope: Envelope| {
        println!("Server handling: {}", envelope.message_type);

        Some(Envelope {
            message_type: "test::Pong".to_string(),
            payload: b"pong".to_vec(),
            correlation_id: envelope.correlation_id,
            sender_node: "server".to_string(),
            target_actor: envelope.sender_node.clone(),
            is_response: true,
        })
    });

    let server = RemoteServer::bind("127.0.0.1:0", handler).await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let transport = TcpTransport;
    let conn = transport.connect(&addr.to_string()).await.unwrap();
    let client = RemoteClient::new(conn);

    let remote: RemoteAddr<()> = RemoteAddr::new("server-node", "echo-actor", client);

    // Send via RemoteAddr
    register_message::<Ping>();
    let response = remote
        .send(Ping {
            message: "hello".to_string(),
        })
        .await
        .unwrap();

    assert!(response.is_response);
    assert_eq!(response.correlation_id, 1); // First correlation ID
    println!("Got response: {:?}", response.message_type);
}
