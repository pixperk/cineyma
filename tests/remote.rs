use cinema::{
    remote::{
        deserialize_payload, proto::Envelope, register_message, Connection, RemoteMessage,
        TcpConnection, TcpTransport, Transport,
    },
    Message,
};
use prost::Message as ProstMessage;
use tokio::net::TcpListener;

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
