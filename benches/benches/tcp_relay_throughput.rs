use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn relay_legacy_copy(buf: &[u8], rounds: usize) -> usize {
    let mut total = 0usize;
    for _ in 0..rounds {
        let mut dst = Vec::with_capacity(buf.len());
        dst.extend_from_slice(buf);
        total += dst.len();
    }
    total
}

fn relay_v2_ring_copy(buf: &[u8], rounds: usize) -> usize {
    let mut total = 0usize;
    let mut ring = blackwire_common::relay::RelayRingBuffer::new(16 * 1024, 256 * 1024);
    let mut dst = vec![0u8; buf.len()];
    for _ in 0..rounds {
        let mut offset = 0;
        while offset < buf.len() {
            if ring.remaining_capacity() == 0 {
                ring.grow();
            }
            let pushed = ring.push_slice(&buf[offset..]);
            offset += pushed;

            while !ring.is_empty() {
                let front = ring.front_slice();
                let dst_len = dst.len();
                let offset = total % dst_len;
                let n = front.len().min(dst_len - offset);
                dst[offset..offset + n].copy_from_slice(&front[..n]);
                ring.consume(n);
                total += n;
            }
        }
    }
    total
}

fn bench_tcp_relay(c: &mut Criterion) {
    let mut group = c.benchmark_group("tcp_relay_throughput");
    for size in [1024usize, 16 * 1024, 64 * 1024] {
        let payload = vec![0xAB; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("legacy_vec_copy", size),
            &payload,
            |b, p| b.iter(|| relay_legacy_copy(p, 64)),
        );
        group.bench_with_input(BenchmarkId::new("relay_v2_ring", size), &payload, |b, p| {
            b.iter(|| relay_v2_ring_copy(p, 64))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_tcp_relay);
criterion_main!(benches);
