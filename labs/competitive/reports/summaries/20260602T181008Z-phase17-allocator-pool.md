# Phase 17 allocator and buffer-pool summary

Date: 2026-06-02 18:10:08 UTC

## Scope

Implemented the original-plan Phase 17 allocation slice:

- Added opt-in CLI allocator features: `jemalloc` and `mimalloc`.
- Kept the default allocator unchanged.
- Extended the shared `BufferPool` to Phase 17 size classes:
  - `4 KiB` control
  - `16 KiB` default relay
  - `64 KiB` bulk relay
  - `256 KiB` QUIC/Hysteria/TUN bulk
- Added pool metrics:
  - `blackwire_pool_acquire_total{size}`
  - `blackwire_pool_release_total{size}`
  - `blackwire_pool_miss_total{size}`
  - `blackwire_pool_bytes_active`
- Added a focused Criterion benchmark: `cargo bench -p blackwire-benches --bench buffer_pool`.

## Benchmark

Host: local macOS development machine

Command:

```sh
cargo bench -p blackwire-benches --bench buffer_pool
```

Corrected benchmark results after preventing fresh-allocation optimization:

| size | fresh mean | pooled mean | read |
| ---: | ---: | ---: | --- |
| `4096` | `21.819 us` | `38.746 us` | pooled slower |
| `16384` | `66.794 us` | `79.821 us` | pooled slower |
| `65536` | `350.05 us` | `316.21 us` | pooled faster |
| `262144` | `1.3187 ms` | `1.2449 ms` | pooled faster |

## Acceptance read

- The implementation is functional and measurable.
- The allocator replacement features compile but remain opt-in.
- The pool improvement is accepted only for bulk classes in this synthetic bench:
  - `64 KiB` improved allocation-loop time by roughly `9.7%`.
  - `256 KiB` improved allocation-loop time by roughly `5.6%`.
- Small/control classes did not improve in the synthetic bench, so this phase should not claim a universal allocation win.
- Default runtime behavior remains conservative: no alternate allocator is enabled by default, and future use of the new `256 KiB` pool class should be limited to bulk/QUIC/TUN paths that show a real win.
