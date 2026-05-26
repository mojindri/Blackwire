mod protocol_matrix;

use bench_harness::ProtocolPath;
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_matrix(c: &mut Criterion) {
    protocol_matrix::register_protocol_benches(c, ProtocolPath::VlessWs);
}

criterion_group!(e2e_vless_ws, bench_matrix);
criterion_main!(e2e_vless_ws);
