use blackwire_common::{
    BufferPool, BULK_RELAY_BUFFER_SIZE, CONTROL_BUFFER_SIZE, DEFAULT_RELAY_BUFFER_SIZE,
    QUIC_BULK_BUFFER_SIZE,
};
use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

fn fresh_allocate(size: usize, rounds: usize) -> usize {
    let mut total = 0usize;
    for _ in 0..rounds {
        let mut buf = BytesMut::with_capacity(black_box(size));
        buf.resize(black_box(size), 0xA5);
        black_box(&mut buf);
        total += black_box(buf.len());
    }
    total
}

fn pooled_allocate(pool: &BufferPool, size: usize, rounds: usize) -> usize {
    let mut total = 0usize;
    for _ in 0..rounds {
        let mut buf = pool.acquire(black_box(size));
        buf.resize(black_box(size), 0xA5);
        black_box(&mut buf);
        total += black_box(buf.len());
        pool.release(buf);
    }
    total
}

fn bench_buffer_pool(c: &mut Criterion) {
    let pool = BufferPool::new();
    let mut group = c.benchmark_group("buffer_pool");
    for size in [
        CONTROL_BUFFER_SIZE,
        DEFAULT_RELAY_BUFFER_SIZE,
        BULK_RELAY_BUFFER_SIZE,
        QUIC_BULK_BUFFER_SIZE,
    ] {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("fresh", size), &size, |b, &size| {
            b.iter(|| fresh_allocate(size, 256))
        });
        group.bench_with_input(BenchmarkId::new("pooled", size), &size, |b, &size| {
            b.iter(|| pooled_allocate(pool.as_ref(), size, 256))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_buffer_pool);
criterion_main!(benches);
