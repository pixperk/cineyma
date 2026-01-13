//! TCP Chat Server Example
//!
//! Run with: cargo run --example chat
//! Connect with: nc localhost 8080
//!
//! Architecture:
//! - ChatServer: manages connected clients, broadcasts messages
//! - ClientSession: one per TCP connection, reads via stream

use cineyma::{Actor, ActorSystem, Addr, Context, Handler, Message, StreamHandler};
use std::collections::HashMap;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_stream::wrappers::LinesStream;

// ============ Messages ============

struct Connect {
    username: String,
    addr: Addr<ClientSession>,
}

impl Message for Connect {
    type Result = ();
}

struct Disconnect {
    username: String,
}

impl Message for Disconnect {
    type Result = ();
}

struct Broadcast {
    from: String,
    text: String,
}

impl Message for Broadcast {
    type Result = ();
}

struct SendLine(String);

impl Message for SendLine {
    type Result = ();
}

// ============ ChatServer Actor ============

struct ChatServer {
    clients: HashMap<String, Addr<ClientSession>>,
}

impl ChatServer {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    fn broadcast(&self, message: &str) {
        for client in self.clients.values() {
            let _ = client.try_send(SendLine(message.to_string()));
        }
    }
}

impl Actor for ChatServer {
    fn started(&mut self, _ctx: &mut Context<Self>) {
        println!("[server] ChatServer started");
    }
}

impl Handler<Connect> for ChatServer {
    fn handle(&mut self, msg: Connect, _ctx: &mut Context<Self>) {
        println!("[server] {} joined", msg.username);
        self.broadcast(&format!("* {} joined the chat\n", msg.username));
        self.clients.insert(msg.username, msg.addr);
    }
}

impl Handler<Disconnect> for ChatServer {
    fn handle(&mut self, msg: Disconnect, _ctx: &mut Context<Self>) {
        println!("[server] {} left", msg.username);
        self.clients.remove(&msg.username);
        self.broadcast(&format!("* {} left the chat\n", msg.username));
    }
}

impl Handler<Broadcast> for ChatServer {
    fn handle(&mut self, msg: Broadcast, _ctx: &mut Context<Self>) {
        let line = format!("[{}]: {}\n", msg.from, msg.text);
        for (username, client) in &self.clients {
            if username != &msg.from {
                let _ = client.try_send(SendLine(line.clone()));
            }
        }
    }
}

// ============ ClientSession Actor ============

struct ClientSession {
    username: String,
    server: Addr<ChatServer>,
    writer_tx: mpsc::UnboundedSender<String>,
    lines_stream: Option<LinesStream<BufReader<tokio::io::ReadHalf<TcpStream>>>>,
}

impl Actor for ClientSession {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // Add the TCP read stream
        if let Some(stream) = self.lines_stream.take() {
            ctx.add_stream(stream);
        }

        // Register with the chat server
        let _ = self.server.try_send(Connect {
            username: self.username.clone(),
            addr: ctx.address(),
        });
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        let _ = self.server.try_send(Disconnect {
            username: self.username.clone(),
        });
    }
}

impl StreamHandler<Result<String, io::Error>> for ClientSession {
    fn handle(&mut self, item: Result<String, io::Error>, ctx: &mut Context<Self>) {
        match item {
            Ok(line) => {
                let line = line.trim();
                if !line.is_empty() {
                    let _ = self.server.try_send(Broadcast {
                        from: self.username.clone(),
                        text: line.to_string(),
                    });
                }
            }
            Err(e) => {
                println!("[{}] read error: {}", self.username, e);
                ctx.stop();
            }
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        println!("[{}] connection closed", self.username);
        ctx.stop();
    }
}

impl Handler<SendLine> for ClientSession {
    fn handle(&mut self, msg: SendLine, _ctx: &mut Context<Self>) {
        let _ = self.writer_tx.send(msg.0);
    }
}

// ============ Main ============

#[tokio::main]
async fn main() -> io::Result<()> {
    let system = ActorSystem::new();

    // Start the chat server
    let server = system.spawn(ChatServer::new());
    println!("Chat server started on 127.0.0.1:8080");
    println!("Connect with: nc localhost 8080");

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let mut client_counter = 0u32;

    loop {
        let (socket, addr) = listener.accept().await?;
        client_counter += 1;
        let username = format!("user{}", client_counter);
        println!("New connection from {} as {}", addr, username);

        // Split TCP stream
        let (reader, mut writer) = tokio::io::split(socket);

        // Create channel for writes
        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<String>();

        // Spawn writer task (outside actor system - just handles async writes)
        let user_clone = username.clone();
        tokio::spawn(async move {
            while let Some(line) = writer_rx.recv().await {
                if let Err(e) = writer.write_all(line.as_bytes()).await {
                    println!("[{}] write error: {}", user_clone, e);
                    break;
                }
            }
        });

        // Create line stream from reader
        let lines = LinesStream::new(BufReader::new(reader).lines());

        // Create and spawn client session
        let session = ClientSession {
            username,
            server: server.clone(),
            writer_tx,
            lines_stream: Some(lines),
        };

        let _addr = system.spawn(session);
    }
}
