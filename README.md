# Cinema

A lightweight actor model framework for Rust, inspired by Erlang/OTP, Akka, and actix.

## Features

- **Async/await native** - Built on Tokio
- **Typed messages** - Compile-time message safety
- **Supervision** - Restart, Stop, Escalate strategies (OTP-style)
- **Streams** - Process external data streams within actors
- **Timers** - `run_later` and `run_interval` scheduling
- **Registry** - Name-based actor lookup with auto-cleanup
- **Async handlers** - Non-blocking I/O in message handlers
- **Remote actors** - TCP transport with Protocol Buffers serialization
- **Cluster** - Gossip protocol for membership, failure detection, and distributed actor registry

## Design Philosophy

Cinema prioritizes:
- **Explicit supervision** over silent recovery
- **Typed messaging** over dynamic routing
- **Sequential state ownership** over shared concurrency
- **Minimal magic**, maximal control

If you want HTTP-first or macro-heavy ergonomics, use [actix](https://actix.rs/).
If you want OTP-style fault tolerance in Rust, use Cinema.

> **Note on panics:** Cinema treats panics inside actors as failures, similar to Erlang process crashes. Panics are caught at actor boundaries and never crash the runtime.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Core Concepts](#core-concepts)
   - [Context API](#context-api)
   - [Supervision](#supervision)
   - [Streams](#streams)
   - [Registry](#registry)
3. [Remote Actors](#remote-actors)
   - [Basic Remote Messaging](#basic-remote-messaging)
   - [Message Router](#message-router)
4. [Cluster](#cluster)
   - [Gossip Protocol](#gossip-protocol)
   - [Failure Detection](#failure-detection)
   - [Distributed Actor Registry](#distributed-actor-registry)
   - [Cluster-Aware Remote Communication](#cluster-aware-remote-communication)
5. [Examples](#examples)
6. [Architecture](#architecture)
7. [License](#license)

---

## Quick Start

```rust
use cinema::{Actor, Handler, Message, ActorSystem, Context};

// define a message
struct Greet(String);

impl Message for Greet {
    type Result = String;
}

// define an actor
struct Greeter;

impl Actor for Greeter {}

impl Handler<Greet> for Greeter {
    fn handle(&mut self, msg: Greet, _ctx: &mut Context<Self>) -> String {
        format!("Hello, {}!", msg.0)
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let addr = system.spawn(Greeter);

    // fire and forget
    addr.do_send(Greet("World".into()));

    // request-response
    let response = addr.send(Greet("Cinema".into())).await.unwrap();
    println!("{}", response); // "Hello, Cinema!"
}
```

---

## Core Concepts

### Context API

`Context<Self>` is the actor's handle to the runtime:

| Method | Description |
|--------|-------------|
| `spawn_child(actor)` | Spawn supervised child |
| `spawn_child_with_strategy(factory, strategy)` | Spawn with restart policy |
| `stop()` | Stop this actor |
| `address()` | Get own `Addr<Self>` |
| `run_later(duration, msg)` | Delayed self-message |
| `run_interval(duration, msg)` | Periodic self-message |
| `add_stream(stream)` | Attach async stream |
| `watch(addr)` | Get notified when actor dies |

### Supervision

```rust
use cinema::{Actor, Context, SupervisorStrategy};
use std::time::Duration;

struct Parent;
struct Child;

impl Actor for Parent {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // restart child up to 3 times within 10 seconds
        ctx.spawn_child_with_strategy(
            || Child,
            SupervisorStrategy::restart(3, Duration::from_secs(10)),
        );
    }
}

impl Actor for Child {}
```

**Strategies:**
- `Stop` - Let actor die (default)
- `Restart { max_restarts, within }` - Restart on panic, up to N times within duration
- `Escalate` - Propagate failure to parent (OTP-style)

### Streams

```rust
use cinema::{Actor, StreamHandler, Context};
use tokio_stream::wrappers::UnboundedReceiverStream;

struct MyActor {
    stream: Option<UnboundedReceiverStream<i32>>,
}

impl Actor for MyActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Some(stream) = self.stream.take() {
            ctx.add_stream(stream);
        }
    }
}

impl StreamHandler<i32> for MyActor {
    fn handle(&mut self, item: i32, _ctx: &mut Context<Self>) {
        println!("Received: {}", item);
    }

    fn finished(&mut self, _ctx: &mut Context<Self>) {
        println!("Stream completed");
    }
}
```

### Registry

```rust
let system = ActorSystem::new();
let addr = system.spawn(MyActor);

// register with auto-unregister on actor death
system.register("my_actor", addr);

// lookup
if let Some(addr) = system.lookup::<MyActor>("my_actor") {
    addr.do_send(SomeMessage);
}
```

> **Failure semantics:** Registry entries are automatically removed when actors stop. During restarts, the same `Addr` remains valid - senders don't need to re-lookup.

---

## Remote Actors

Cinema supports sending messages to actors on other nodes over TCP with Protocol Buffers serialization.

### Basic Remote Messaging

**Define messages** (protobuf serializable):

```rust
use cinema::{Message, remote::RemoteMessage};
use prost::Message as ProstMessage;

// request message
#[derive(Clone, ProstMessage)]
struct Add {
    #[prost(int32, tag = "1")]
    n: i32,
}
impl Message for Add {
    type Result = AddResult;
}
impl RemoteMessage for Add {}

// response message
#[derive(Clone, ProstMessage)]
struct AddResult {
    #[prost(int32, tag = "1")]
    value: i32,
}
impl Message for AddResult {
    type Result = ();
}
impl RemoteMessage for AddResult {}
```

**Server side:**

```rust
use cinema::{Actor, Handler, ActorSystem, Context};
use cinema::remote::{LocalNode, RemoteServer};

struct Calculator { value: i32 }

impl Actor for Calculator {}

impl Handler<Add> for Calculator {
    fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> AddResult {
        self.value += msg.n;
        AddResult { value: self.value }
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let calc = system.spawn(Calculator { value: 0 });

    let node = LocalNode::new("calc-server");
    let handler = node.handler::<Calculator, Add>(calc);

    let server = RemoteServer::bind("0.0.0.0:8080", handler).await.unwrap();
    server.run().await;
}
```

**Client side:**

```rust
use cinema::remote::{RemoteClient, TcpTransport, Transport};

#[tokio::main]
async fn main() {
    let transport = TcpTransport;
    let conn = transport.connect("127.0.0.1:8080").await.unwrap();
    let client = RemoteClient::new(conn);

    let remote = client.remote_addr::<Calculator>("calc-server", "calculator");

    let response = remote.send(Add { n: 5 }).await.unwrap();
    let result = AddResult::decode(response.payload.as_slice()).unwrap();
    println!("Result: {}", result.value);
}
```

### Message Router

Handle multiple message types:

```rust
use cinema::remote::MessageRouter;

let handler = MessageRouter::new()
    .route::<Add>(node.handler::<Calculator, Add>(calc.clone()))
    .route::<Subtract>(node.handler::<Calculator, Subtract>(calc.clone()))
    .route::<GetValue>(node.handler::<Calculator, GetValue>(calc))
    .build();

let server = RemoteServer::bind("0.0.0.0:8080", handler).await.unwrap();
```

---

## Cluster

Cinema provides a gossip-based cluster with:
- **Membership management** - Track which nodes are in the cluster
- **Failure detection** - Mark nodes as SUSPECT/DOWN based on heartbeat
- **Distributed actor registry** - Discover which node hosts an actor
- **Location-transparent messaging** - Send to actors by name, not node address

### Gossip Protocol

Nodes exchange membership information to achieve eventual consistency:

```rust
use cinema::remote::cluster::{ClusterNode, Node, NodeStatus};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // create cluster node
    let node1 = Arc::new(ClusterNode::new(
        "node-1".to_string(),
        "127.0.0.1:7001".to_string(),
    ));

    // start gossip server
    tokio::spawn(node1.clone().start_gossip_server(7001));

    // add peer and start periodic gossip
    node1.add_member(Node {
        id: "node-2".to_string(),
        addr: "127.0.0.1:7002".to_string(),
        status: NodeStatus::Up,
    }).await;

    // gossip every 100ms, mark suspect after 5s
    node1.clone().start_periodic_gossip(
        Duration::from_millis(100),
        Duration::from_secs(5),
    );

    // after multiple rounds, all nodes converge to same membership view
}
```

### Failure Detection

Nodes track heartbeat timestamps and mark unresponsive nodes:

- **Up** → **Suspect** after `suspect_timeout`
- **Suspect** → **Down** after `suspect_timeout * 2`

```rust
// node becomes SUSPECT if no gossip for 5s
// node becomes DOWN if no gossip for 10s
node.start_periodic_gossip(
    Duration::from_millis(100),
    Duration::from_secs(5),
);
```

### Distributed Actor Registry

Actors register on their local node, and their location spreads via gossip:

```rust
// node2 registers an actor
node2.register_actor("user-store".to_string(), "UserStore".to_string()).await;

// after gossip propagates, node1 can lookup the actor
let location = node1.lookup_actor("user-store").await;
// Some(("node-2", "UserStore"))
```

**Actor cleanup:** When a node goes DOWN, all its actors are removed from the registry.

### Cluster-Aware Remote Communication

`ClusterClient` combines cluster discovery with remote messaging:

**Server setup:**

```rust
use cinema::{Actor, Handler, ActorSystem, Context};
use cinema::remote::{LocalNode, MessageRouter};
use cinema::remote::cluster::ClusterNode;
use std::sync::Arc;

// actor
struct PingPong;
impl Actor for PingPong {}

#[derive(Clone, prost::Message)]
struct Ping { #[prost(string, tag = "1")] msg: String }
impl Message for Ping { type Result = Pong; }
impl RemoteMessage for Ping {}

#[derive(Clone, prost::Message)]
struct Pong { #[prost(string, tag = "1")] reply: String }
impl Message for Pong { type Result = (); }
impl RemoteMessage for Pong {}

impl Handler<Ping> for PingPong {
    fn handle(&mut self, msg: Ping, _ctx: &mut Context<Self>) -> Pong {
        Pong { reply: format!("pong: {}", msg.msg) }
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let actor = system.spawn(PingPong);

    // create cluster node
    let node = Arc::new(ClusterNode::new(
        "node-2".to_string(),
        "127.0.0.1:9002".to_string(),
    ));

    // register actor in cluster
    node.register_actor("pingpong".to_string(), "PingPong".to_string()).await;

    // create handler
    let local_node = LocalNode::new("node-2");
    let handler = MessageRouter::new()
        .route::<Ping>(local_node.handler::<PingPong, Ping>(actor))
        .build();

    // start unified server (gossip + actor messages)
    tokio::spawn(node.clone().start_server(9002, Some(handler)));

    // start periodic gossip
    node.clone().start_periodic_gossip(
        Duration::from_millis(100),
        Duration::from_secs(10),
    );
}
```

**Client usage:**

```rust
use cinema::remote::{ClusterClient, ClusterRemoteAddr};

// create cluster client
let client = ClusterClient::new(node1.clone());

// create typed remote address (no manual node lookup needed!)
let remote: ClusterRemoteAddr<PingPong> = client.remote_addr("pingpong");

// option 1: low-level send (returns envelope)
let envelope = remote.send(Ping { msg: "hello" }).await?;
let pong = Pong::decode(envelope.payload.as_slice())?;

// option 2: high-level call (auto-decodes response) ⭐
let pong: Pong = remote.call(Ping { msg: "hello" }).await?;
println!("{}", pong.reply); // "pong: hello"

// option 3: fire-and-forget
remote.do_send(Ping { msg: "notify" }).await?;
```

**How it works:**

1. `remote.call(Ping { ... })` looks up "pingpong" in cluster registry → finds node-2
2. Gets/creates RemoteClient connection to node-2
3. Wraps message in ClusterMessage (multiplexed protocol)
4. Sends with correlation ID tracking for concurrent requests
5. Receives response and auto-decodes to typed `Pong`

**Features:**

- ✅ **Location transparency** - send by actor name, not node address
- ✅ **Automatic discovery** - cluster registry finds the actor's node
- ✅ **Connection pooling** - reuses connections per node
- ✅ **Concurrent requests** - multiple requests multiplexed over one connection
- ✅ **Type-safe API** - compile-time checked request/response types
- ✅ **Error recovery** - failed connections auto-removed and recreated

---

## Examples

### CRUD Example

A simple in-memory user store - **no locks needed!**

```rust
use cinema::{Actor, Handler, Message, ActorSystem, Context};
use std::collections::HashMap;

// messages
struct CreateUser { id: u64, name: String }
struct GetUser { id: u64 }
struct UpdateUser { id: u64, name: String }
struct DeleteUser { id: u64 }

impl Message for CreateUser { type Result = (); }
impl Message for GetUser { type Result = Option<String>; }
impl Message for UpdateUser { type Result = bool; }
impl Message for DeleteUser { type Result = bool; }

// actor
struct UserStore {
    users: HashMap<u64, String>,
}

impl Actor for UserStore {}

impl Handler<CreateUser> for UserStore {
    fn handle(&mut self, msg: CreateUser, _ctx: &mut Context<Self>) {
        self.users.insert(msg.id, msg.name);
    }
}

impl Handler<GetUser> for UserStore {
    fn handle(&mut self, msg: GetUser, _ctx: &mut Context<Self>) -> Option<String> {
        self.users.get(&msg.id).cloned()
    }
}

impl Handler<UpdateUser> for UserStore {
    fn handle(&mut self, msg: UpdateUser, _ctx: &mut Context<Self>) -> bool {
        if self.users.contains_key(&msg.id) {
            self.users.insert(msg.id, msg.name);
            true
        } else {
            false
        }
    }
}

impl Handler<DeleteUser> for UserStore {
    fn handle(&mut self, msg: DeleteUser, _ctx: &mut Context<Self>) -> bool {
        self.users.remove(&msg.id).is_some()
    }
}
```

### TCP Chat Server

Run the chat server example:

```bash
cargo run -p chat
```

Connect with netcat:

```bash
nc localhost 8080
```

See [examples/chat/README.md](examples/chat/README.md) for details.

### Distributed Key-Value Store

demonstrates cluster capabilities: gossip membership, actor discovery, location-transparent messaging, and failure detection.

```bash
# start first node
cargo run -p distributed-kv -- node-1

# start second node (joins via first)
cargo run -p distributed-kv -- node-2 127.0.0.1:7001

# start third node
cargo run -p distributed-kv -- node-3 127.0.0.1:7001
```

interactive cli commands:
```
> set user:1 alice
OK

> get user:1
alice

> members
cluster members:
  node-1 @ 127.0.0.1:7001 [Up]
  node-2 @ 127.0.0.1:7002 [Up]
  node-3 @ 127.0.0.1:7003 [Up]

> actors
registered actors:
  kv-store -> node-1 (KVStore)
```

See [examples/distributed-kv/README.md](examples/distributed-kv/README.md) for details.

---

## Architecture

### Local Actor System

```mermaid
graph TB
    subgraph ActorSystem
        AS[ActorSystem]
        REG[Registry]
        AS --> REG
    end

    subgraph Actors
        A1[Actor A]
        A2[Actor B]
        A3[Actor C]
    end

    AS -->|spawn| A1
    AS -->|spawn| A2
    A1 -->|spawn_child| A3

    subgraph "Message Flow"
        M1[Message]
        MB1[Mailbox]
        H1[Handler]
    end

    M1 -->|do_send/send| MB1
    MB1 -->|deliver| H1
```

### Remote Messaging

```mermaid
graph TB
    subgraph "Client Node"
        RA[RemoteAddr]
        RC[RemoteClient]
        RC -->|creates| RA
    end

    subgraph "Network"
        TCP[TCP Connection]
        ENV[Envelope<br/>protobuf bytes]
    end

    subgraph "Server Node"
        LN2[LocalNode]
        RS[RemoteServer]
        MR[MessageRouter]
        H[Handler]
        ADDR[Addr]
        ACT[Actor]

        LN2 -->|creates| H
        RS -->|dispatches to| MR
        MR -->|routes to| H
        H -->|calls| ADDR
        ADDR -->|sends to| ACT
    end

    RC -->|sends| TCP
    TCP -->|delivers| RS
    ACT -->|returns| H
    H -->|responds via| TCP
    TCP -->|receives| RC

    style RC fill:#4a9eff
    style LN2 fill:#4a9eff
    style ACT fill:#22c55e
```

### Cluster Architecture

```mermaid
graph TB
    subgraph "Node 1"
        CN1[ClusterNode]
        CC1[ClusterClient]
        A1[Actor A]

        CN1 -->|provides registry| CC1
        CC1 -->|sends to| A1
    end

    subgraph "Node 2"
        CN2[ClusterNode]
        A2[Actor B]
        A3[Actor C]

        CN2 -->|hosts| A2
        CN2 -->|hosts| A3
    end

    subgraph "Node 3"
        CN3[ClusterNode]
        A4[Actor D]

        CN3 -->|hosts| A4
    end

    CN1 <-->|gossip| CN2
    CN2 <-->|gossip| CN3
    CN1 <-->|gossip| CN3

    CC1 -.->|"1. lookup 'ActorB'"| CN1
    CN1 -.->|"2. node-2"| CC1
    CC1 ==>|"3. send msg"| A2

    style CN1 fill:#4a9eff
    style CN2 fill:#4a9eff
    style CN3 fill:#4a9eff
    style CC1 fill:#22c55e
```

**Multiplexed Protocol:**

```mermaid
graph LR
    subgraph "ClusterMessage (oneof)"
        G[Gossip]
        E[Envelope<br/>Actor Message]
    end

    subgraph "Single Port"
        S[Server]
    end

    G -->|route to| S
    E -->|route to| S

    S -->|gossip handler| M[Membership Merge]
    S -->|actor handler| A[Actor Dispatch]

    style G fill:#fbbf24
    style E fill:#22c55e
```

---

## Roadmap

- [x] Core Actors
- [x] Supervision
- [x] Streams
- [x] Registry
- [x] Remote Actors
- [x] Cluster Gossip
- [x] Failure Detection
- [x] Distributed Actor Registry
- [x] Cluster-Aware Messaging

**Coming Soon:**
- [ ] Cluster sharding
- [ ] Persistent actors (event sourcing)
- [ ] Distributed sagas

---

## License

MIT
