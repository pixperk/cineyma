//! # cineyma
//!
//! A lightweight, production-focused actor framework for Rust.
//!
//! cineyma provides typed actors with bounded mailboxes, OTP-style supervision,
//! and distributed messaging over TCP with Protocol Buffers.
//!
//! ## Quick Start
//!
//! ```rust
//! use cineyma::{Actor, Handler, Message, ActorSystem, Context};
//!
//! // Define a message
//! struct Greet(String);
//! impl Message for Greet {
//!     type Result = String;
//! }
//!
//! // Define an actor
//! struct Greeter;
//! impl Actor for Greeter {}
//!
//! impl Handler<Greet> for Greeter {
//!     fn handle(&mut self, msg: Greet, _ctx: &mut Context<Self>) -> String {
//!         format!("Hello, {}!", msg.0)
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let system = ActorSystem::new();
//!     let addr = system.spawn(Greeter);
//!
//!     // Request-response
//!     let response = addr.send(Greet("cineyma".into())).await.unwrap();
//!     assert_eq!(response, "Hello, cineyma!");
//! }
//! ```
//!
//! ## Core Concepts
//!
//! | Concept | Description |
//! |---------|-------------|
//! | [`Actor`] | Trait for stateful message processors |
//! | [`Message`] | Trait for typed messages with associated result |
//! | [`Handler<M>`](Handler) | Sync message handler implementation |
//! | [`Addr<A>`](Addr) | Handle to send messages to an actor |
//! | [`Context<A>`](Context) | Actor's runtime environment |
//! | [`ActorSystem`] | Top-level runtime that spawns actors |
//!
//! ## Sending Messages
//!
//! ```rust,ignore
//! // Request-response (awaits result)
//! let result = addr.send(MyMessage).await?;
//!
//! // Fire-and-forget with backpressure
//! addr.do_send(MyMessage).await?;
//!
//! // Non-blocking (returns MailboxFull if full)
//! addr.try_send(MyMessage)?;
//! ```
//!
//! ## Bounded Mailboxes
//!
//! All actors have bounded mailboxes (default: 256 messages) to prevent OOM:
//!
//! ```rust,ignore
//! // Default capacity
//! let addr = system.spawn(MyActor);
//!
//! // Custom capacity
//! let addr = system.spawn_with_capacity(MyActor, 10000);
//! ```
//!
//! ## Supervision
//!
//! Actors can supervise children with restart policies:
//!
//! ```rust,ignore
//! use cineyma::SupervisorStrategy;
//! use std::time::Duration;
//!
//! // Restart up to 3 times within 10 seconds
//! ctx.spawn_child_with_strategy(
//!     || MyChild,
//!     SupervisorStrategy::restart(3, Duration::from_secs(10)),
//! );
//! ```
//!
//! ## Modules
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`actor`] | Actor and Handler traits |
//! | [`address`] | Actor addressing and message sending |
//! | [`context`] | Runtime context with child spawning, timers, streams |
//! | [`system`] | ActorSystem for top-level actor management |
//! | [`supervisor`] | Supervision strategies (Stop, Restart, Escalate) |
//! | [`registry`] | Named actor lookup with auto-cleanup |
//! | [`stream`] | External stream integration |
//! | [`remote`] | TCP transport, clustering, and distributed actors |
//! | [`error`] | Error types ([`MailboxError`]) |

pub mod actor;
pub mod address;
pub mod context;
pub mod envelope;
pub mod error;
pub mod message;
pub mod registry;
pub mod remote;
pub mod stream;
pub mod supervisor;
pub mod system;
pub mod timer;
pub mod watcher;

pub use actor::{Actor, Handler, StreamHandler};
pub use address::Addr;
pub use context::Context;
pub use error::MailboxError;
pub use message::Message;
pub use supervisor::SupervisorStrategy;
pub use system::ActorSystem;
pub use timer::TimerHandle;
