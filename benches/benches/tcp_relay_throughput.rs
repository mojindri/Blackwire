use std::hint::black_box;

use blackwire_common::relay::{
    copy_bidirectional_pooled, copy_bidirectional_v2, RelayFlushPolicy, RelayV2Options,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Copy)]
enum Engine {
    Legacy,
    V2,
}

async fn relay_round_trip(engine: Engine, payload: &[u8]) -> usize {
    let capacity = (payload.len() * 2).max(1024);
    let (client_a, relay_a) = tokio::io::duplex(capacity);
    let (relay_b, client_b) = tokio::io::duplex(capacity);
    let relay = tokio::spawn(async move {
        match engine {
            Engine::Legacy => copy_bidirectional_pooled(relay_a, relay_b).await,
            Engine::V2 => copy_bidirectional_v2(
                relay_a,
                relay_b,
                RelayV2Options {
                    initial_buffer: 16 * 1024,
                    max_buffer: 256 * 1024,
                    flush_policy: RelayFlushPolicy::Adaptive,
                },
            )
            .await
            .map(|stats| stats.byte_totals()),
        }
    });

    let (mut a_read, mut a_write) = tokio::io::split(client_a);
    let (mut b_read, mut b_write) = tokio::io::split(client_b);
    let a_payload = payload.to_vec();
    let b_payload = payload.to_vec();
    let send_a = async move {
        a_write.write_all(&a_payload).await.unwrap();
        a_write.shutdown().await.unwrap();
    };
    let send_b = async move {
        b_write.write_all(&b_payload).await.unwrap();
        b_write.shutdown().await.unwrap();
    };
    let read_a = async move {
        let mut received = Vec::with_capacity(payload.len());
        a_read.read_to_end(&mut received).await.unwrap();
        received.len()
    };
    let read_b = async move {
        let mut received = Vec::with_capacity(payload.len());
        b_read.read_to_end(&mut received).await.unwrap();
        received.len()
    };
    let (_, _, received_a, received_b) = tokio::join!(send_a, send_b, read_a, read_b);
    relay.await.unwrap().unwrap();
    black_box(received_a + received_b)
}

async fn relay_batch(engine: Engine, payload: &[u8], concurrency: usize) -> usize {
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..concurrency {
        let payload = payload.to_vec();
        tasks.spawn(async move { relay_round_trip(engine, &payload).await });
    }
    let mut total = 0;
    while let Some(result) = tasks.join_next().await {
        total += result.unwrap();
    }
    black_box(total)
}

fn bench_tcp_relay(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut group = c.benchmark_group("tcp_relay_throughput");
    for size in [1024usize, 16 * 1024, 64 * 1024, 1024 * 1024] {
        let payload = vec![0xAB; size];
        group.throughput(Throughput::Bytes((size * 2) as u64));
        for (name, engine) in [("legacy", Engine::Legacy), ("v2_adaptive", Engine::V2)] {
            group.bench_with_input(BenchmarkId::new(name, size), &payload, |b, p| {
                b.iter(|| runtime.block_on(relay_round_trip(engine, black_box(p))))
            });
        }
    }
    group.finish();

    let mut concurrent = c.benchmark_group("tcp_relay_concurrent_8");
    for size in [1024usize, 64 * 1024] {
        let payload = vec![0xAB; size];
        concurrent.throughput(Throughput::Bytes((size * 2 * 8) as u64));
        for (name, engine) in [("legacy", Engine::Legacy), ("v2_adaptive", Engine::V2)] {
            concurrent.bench_with_input(BenchmarkId::new(name, size), &payload, |b, p| {
                b.iter(|| runtime.block_on(relay_batch(engine, black_box(p), 8)))
            });
        }
    }
    concurrent.finish();
}

criterion_group!(benches, bench_tcp_relay);
criterion_main!(benches);
