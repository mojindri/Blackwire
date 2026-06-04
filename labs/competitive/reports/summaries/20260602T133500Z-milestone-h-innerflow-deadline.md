# Milestone H: InnerFlow and Deadline Scheduler

Date: 2026-06-02

## Scope

Milestone H covers:

- inner-flow classifier
- FQ/DRR-style scheduling primitive
- deadline classes
- packet class detection
- bulk + DNS/game/interactive benchmark coverage

## Implementation Result

Implemented in this pass:

- Added `blackwire-transport::innerflow`.
- Added `PacketClass` with `Control`, `Dns`, `Interactive`, `WebFirstByte`, and `Bulk`.
- Added `InnerFlowKey`, `InnerFlowPacket`, and `InnerFlowScheduler`.
- Scheduler dequeues sparse/latency-sensitive classes before bulk:
  - control
  - DNS
  - interactive
  - web first-byte
  - bulk
- Added class deadlines:
  - control: 50 ms
  - DNS: 100 ms
  - interactive: 80 ms
  - web first-byte: 150 ms
  - bulk: no deadline
- Added stale packet dropping before dequeue.
- Added bulk queue cap behavior.
- Added Hysteria2 UDP packet classification:
  - destination port `53` / `5353` -> DNS
  - small non-DNS payloads -> interactive
  - larger UDP payloads -> bulk
- Wired Hysteria2 UDP server responses through the InnerFlow scheduler before QUIC DATAGRAM send.
- Preserved FEC follow-up datagrams so parity follows its protected packet.
- Added metric descriptors:
  - `blackwire_innerflow_queue_delay_ms`
  - `blackwire_innerflow_drops_total`
  - `blackwire_innerflow_dequeued_total`
  - `blackwire_innerflow_bulk_starvation_prevented_total`

## Validation

Local validation:

- `cargo fmt --all`
- `cargo check -q`
- `cargo test -p blackwire-transport innerflow -- --nocapture`
- `cargo test -p blackwire-transport hysteria2::udp::tests -- --nocapture`
- `cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture`

Remote sanity validation:

- Server VPS: `<server-host>`
- Client VPS: `<client-host>`
- SSH key: `id_hetzner`
- Scenario: `hysteria2-udp-dns-loss-5-innerflow`
- Loss: 5%
- Probe count: 200
- Payload: 64 bytes
- Timeout: 500 ms
- Concurrency: 1
- Raw report: `labs/competitive/reports/hysteria2-udp-dns-loss-5-innerflow-remote-20260602T133345Z.jsonl`

Remote sanity result:

| Variant | OK / 200 | Errors | Stale | RPS | p99 ms |
|---|---:|---:|---:|---:|---:|
| Blackwire standard | 173 | 27 | 0 | 12.74 | 0.868 |
| Blackwire H2-plus + InnerFlow | 170 | 30 | 0 | 11.27 | 0.909 |
| Hysteria | 175 | 25 | 0 | 13.91 | 1.108 |

The scheduled Hysteria2 UDP response path stayed functional under 5% loss and still beat Hysteria p99 in this sanity row.

Remote acceptance validation:

- Scenario: `hysteria2-innerflow-bulk-dns-clean`
- Raw report: `labs/competitive/reports/hysteria2-innerflow-bulk-dns-clean-remote-20260602T140212Z.jsonl`
- Current server: `target/linux-amd64/blackwire`
- Candidate server/client: `target/linux-amd64/blackwire-candidate-milestone-h`
- Concurrency: 64
- DNS probes: 200
- Interactive probes: 200
- Bulk probes: 800
- Bulk payload: 1200 bytes

| Variant | DNS p99 ms | Interactive p99 ms | Bulk RPS | DNS errors | Interactive errors | Bulk errors |
|---|---:|---:|---:|---:|---:|---:|
| Blackwire current | 2.618 | 2.502 | 27487.97 | 0 | 0 | 0 |
| Blackwire InnerFlow candidate | 0.761 | 0.685 | 27532.97 | 0 | 0 | 0 |

Acceptance calculation:

- DNS p99 under bulk improved `70.9%`.
- Interactive UDP p99 under bulk improved `72.6%`.
- Bulk throughput loss was `-0.2%`, meaning candidate bulk RPS was slightly higher than current.

## Acceptance Status

Milestone H is fully accepted for the stated gate:

- DNS p99 under bulk target: `>= 30%`; observed `70.9%`.
- Interactive UDP p99 under bulk target: `>= 25%`; observed `72.6%`.
- Bulk throughput loss target: `<= 10%`; observed `-0.2%`.

Result: accepted.
