#![allow(dead_code)]

use cineyma::{
    actor::{AsyncHandler, BoxFuture},
    Actor, Context, Handler, Message,
};
use prost::Message as ProstMessage;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// ===== SIMPLE ACTORS FOR BENCHMARKING =====

/// no-op actor for pure spawn/throughput tests
pub struct NoOpActor;
impl Actor for NoOpActor {}

/// counter message - fire and forget
pub struct Count;
impl Message for Count {
    type Result = ();
}

/// counter actor tracks message count
pub struct CounterActor {
    pub count: Arc<AtomicUsize>,
}

impl Actor for CounterActor {}

impl Handler<Count> for CounterActor {
    fn handle(&mut self, _msg: Count, _ctx: &mut Context<Self>) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

/// add message - request/response
pub struct Add(pub u32, pub u32);
impl Message for Add {
    type Result = u32;
}

pub struct Calculator;
impl Actor for Calculator {}

impl Handler<Add> for Calculator {
    fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> u32 {
        msg.0.wrapping_add(msg.1)
    }
}

/// async computation actor
pub struct AsyncCompute(pub u32);
impl Message for AsyncCompute {
    type Result = u64;
}

pub struct AsyncActor;
impl Actor for AsyncActor {}

impl AsyncHandler<AsyncCompute> for AsyncActor {
    fn handle(&mut self, msg: AsyncCompute, _ctx: &mut Context<Self>) -> BoxFuture<'_, u64> {
        Box::pin(async move {
            // simulate async work
            tokio::time::sleep(std::time::Duration::from_micros(10)).await;
            (msg.0 as u64) * 2
        })
    }
}

// ===== REMOTE BENCHMARK MESSAGES =====

#[derive(Clone, ProstMessage)]
pub struct BenchPing {
    #[prost(uint64, tag = "1")]
    pub value: u64,
}

impl Message for BenchPing {
    type Result = ();
}

impl cineyma::remote::RemoteMessage for BenchPing {}

#[derive(Clone, ProstMessage)]
pub struct BenchRequest {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(bytes, tag = "2")]
    pub payload: Vec<u8>,
}

impl Message for BenchRequest {
    type Result = BenchResponse;
}

impl cineyma::remote::RemoteMessage for BenchRequest {}

#[derive(Clone, ProstMessage)]
pub struct BenchResponse {
    #[prost(uint64, tag = "1")]
    pub request_id: u64,
    #[prost(bytes, tag = "2")]
    pub payload: Vec<u8>,
}

impl Message for BenchResponse {
    type Result = ();
}

impl cineyma::remote::RemoteMessage for BenchResponse {}

// ===== TEST HELPERS =====

/// create payload of specific size for serialization benchmarks
pub fn create_payload(size_bytes: usize) -> Vec<u8> {
    vec![0x42; size_bytes]
}

/// port allocation helper to avoid conflicts
pub fn get_bench_port(offset: u16) -> u16 {
    19000 + offset
}
