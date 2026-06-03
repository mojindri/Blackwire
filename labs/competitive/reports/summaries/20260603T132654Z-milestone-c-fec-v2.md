# Milestone C - FEC 2.0 Duplicate-Safe Reliability

Date: 2026-06-03  
Branch: `feature/log_readme`  
Base commit while testing: `41cac86b20ef6da6b7ec4b28319529c5eb86b756`

## Scope

Implemented the Milestone C FEC 2.0 reliability path for Hysteria2 UDP datagrams:

- Duplicate-safe recovery window to suppress late originals after FEC recovery.
- Sequential-DNS guard: FEC is skipped for single-flow sequential DNS until there is enough concurrent DNS flow activity.
- Configurable FEC generation delay, recovery deadline, generation packet count, concurrency threshold, and dedup window.
- Overhead-aware effective group sizing.
- Compact homogeneous-payload parity format for DNS-style groups: protects UDP payload bytes and carries the destination once instead of protecting the full encoded UDP datagram frame.
- Fixed-length compact parity format for equal-size DNS payload groups, storing one payload length instead of per-slot lengths.
- Decoder retry on late originals after parity has already arrived, so parity-first ordering can still recover once enough originals arrive.
- Client-side FEC counters surfaced in UDP benchmark JSON rows.
- Legacy FEC marker decode compatibility retained for the existing `__blackwire_fec_v1__` marker.
- Metrics surfaced for FEC overhead, selected FEC mode, stale drops, recovered packets, and duplicate-safe skips.

Touched code:

- `crates/blackwire-transport/src/hysteria2/udp.rs`
- `crates/blackwire-config/src/schema.rs`
- `crates/blackwire-core/src/hysteria2.rs`
- `crates/blackwire-app/src/metrics.rs`
- `crates/blackwire-cli/src/main.rs`

## Config Surface

Milestone C added or wired these FEC settings:

- `fec.mode`
- `fec.maxOverheadPercent`
- `fec.disableForSequentialDns`
- `fec.minConcurrencyForBlockFec`
- `fec.maxGenerationPackets`
- `fec.maxGenerationDelayMs`
- `fec.recoveryDeadlineMs`
- `fec.dedupWindowPackets`

Defaults are conservative: sequential DNS protection is disabled until concurrent DNS activity is observed, recovery deadline defaults to 100ms, generation delay defaults to 20ms, and dedup window defaults to 1024 packets.

## Local Verification

Commands run:

```bash
cargo test -p blackwire-transport fec -- --nocapture
cargo test -p blackwire-config fec -- --nocapture
cargo check -p blackwire -p blackwire-core -p blackwire-transport
```

Result:

- Transport FEC focused tests passed: 7/7.
- Config FEC focused tests passed: 2/2.
- Compile check passed for CLI, core, and transport.

Focused test coverage includes:

- XOR and Reed-Solomon one-missing-packet recovery.
- Auto FEC conservative default behavior.
- Sequential DNS FEC skip until concurrency threshold.
- Interactive recovery deadline stale drop.
- Late original dedup after FEC recovery.
- Tiny 64-byte DNS packet wire-overhead accounting.

## VPS Benchmark

VPS inventory:

- Server: `<server-host>`
- Client: `<client-host>`
- Native candidate binary built on the server and copied to both benchmark sides.

Command shape:

```bash
cd labs/competitive
export COMPETITIVE_SERVER_HOST=<server-host>
export COMPETITIVE_CLIENT_HOST=<client-host>
REPORT_DIR=reports \
COMPETITIVE_MODE=remote \
LOSS_PERCENT=3 \
HYSTERIA2_UDP_CONCURRENCY=64 \
HYSTERIA2_UDP_COUNT=500 \
HYSTERIA2_UDP_PAYLOAD_BYTES=64 \
HYSTERIA2_CANDIDATE_FEC_MODE=reed-solomon \
BLACKWIRE_CANDIDATE_BIN=$PWD/../../target/linux-amd64/blackwire-candidate-fec-v2 \
BLACKWIRE_CANDIDATE_REMOTE_BIN=/root/blackwire-fec-v2-target/release/blackwire \
COMPETITIVE_SSH_KEY=$PWD/../../id_hetzner \
TMPDIR=$PWD/../../target/tmp \
bash scripts/run_matrix.sh hysteria2-udp-dns-loss-3
```

Final compact-FEC report:

- `labs/competitive/reports/hysteria2-udp-dns-loss-3-remote-20260603T132654Z.jsonl`
- `labs/competitive/reports/blackwire-candidate-h2plus-udp-server-metrics-20260603T132654Z.log`

Final row results:

| Variant | OK / 500 | Errors | p50 ms | p95 ms | p99 ms | Stale |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blackwire current Hysteria2 UDP | 426 | 74 | 1.273 | 1.722 | 1.762 | 0 |
| Blackwire candidate H2+ UDP + FEC v2 compact | 439 | 61 | 1.389 | 1.975 | 2.019 | 0 |
| Hysteria UDP baseline | 466 | 34 | 1.909 | 3.497 | 3.636 | 0 |

Candidate FEC metrics:

```text
blackwire_fec_overhead_bytes_total 5546
blackwire_fec_mode_total{mode="reed-solomon"} 47
blackwire_datagram_packets_total{class="priority",direction="tx"} 487
blackwire_datagram_packets_total{class="priority",direction="rx"} 487
```

Overhead check:

- Application payload bytes: `500 * 64 = 32000`.
- FEC overhead bytes: `5546`.
- Observed overhead: `17.3%`.
- This satisfies the 20% overhead cap in the final compact-FEC row.

## Acceptance Verdict

Milestone C implementation is functionally in, native-built, and VPS-tested, but the current acceptance gate is not fully satisfied.

What passed:

- Duplicate-safe FEC implementation exists and is covered by focused tests.
- Config and CLI surface are wired.
- Native VPS candidate runs.
- Final compact-FEC row keeps overhead under 20%.
- Final compact-FEC row improved Blackwire-current reliability in that run: `61` errors vs `74`.
- No stale replies were observed.

What did not fully pass:

- Candidate p99 was worse than Blackwire current in the final row: `2.019ms` vs `1.762ms`.
- Candidate still had more errors than Hysteria baseline: `61` vs `34`.
- Recovery counter was not observed in the captured server metrics for this row; the loss direction may be affecting the client-side recovery path, but the current harness only captured server metrics for this UDP row.

Conclusion: keep the code as a Milestone C candidate, but do not mark the milestone fully accepted yet. The next validation/fix should capture client-side FEC recovery metrics and tune FEC activation so it improves both p99 and error rate under the same 3-10% lossy UDP/DNS gate.

## Follow-Up Fix Attempt

Additional code changes after the first compact-FEC run:

- Added `Hysteria2UdpSession::fec_snapshot()` so short-lived CLI UDP benchmark clients can report FEC counters in their JSON result row.
- Added client JSON fields:
  - `fec_client_parity_packets`
  - `fec_client_overhead_bytes`
  - `fec_client_recovered_packets`
  - `fec_client_stale_drops`
  - `fec_client_duplicate_safe_skips`
- Switched tiny DNS/interactive FEC from Reed-Solomon to XOR internally while preserving the configured `fec.mode` field in the row. Server metrics now report `blackwire_fec_mode_total{mode="xor1-of-n"}` for these DNS-sized packets.
- Added fixed-length compact FEC payloads for equal-size DNS payload groups.
- Fixed parity-first decode ordering: when parity arrives before the final originals, recovery is retried when later originals fill the decode group.

### 3% Loss Rerun

Report:

- `labs/competitive/reports/hysteria2-udp-dns-loss-3-remote-20260603T170849Z.jsonl`
- `labs/competitive/reports/blackwire-candidate-h2plus-udp-server-metrics-20260603T170849Z.log`

| Variant | OK / 500 | Errors | p50 ms | p95 ms | p99 ms | Stale |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blackwire current Hysteria2 UDP | 447 | 53 | 1.569 | 2.068 | 2.120 | 0 |
| Blackwire candidate H2+ UDP + FEC v2 | 468 | 32 | 0.983 | 1.863 | 2.080 | 0 |
| Hysteria UDP baseline | 462 | 38 | 1.139 | 3.883 | 4.567 | 0 |

Candidate client FEC counters:

```text
fec_client_parity_packets 62
fec_client_overhead_bytes 6200
fec_client_recovered_packets 1
fec_client_stale_drops 0
fec_client_duplicate_safe_skips 0
```

Candidate server FEC counters:

```text
blackwire_fec_mode_total{mode="xor1-of-n"} 57
blackwire_fec_recovered_packets_total 3
blackwire_fec_overhead_bytes_total 5700
```

3% result:

- Errors improved vs current: `32` vs `53`.
- p99 improved vs current: `2.080ms` vs `2.120ms`.
- Overhead stayed below 20%: client `6200 / 32000 = 19.4%`, server `5700 / 32000 = 17.8%`.
- This row is directionally green, but p99 margin is small.

### 10% Loss Rerun

Report:

- `labs/competitive/reports/hysteria2-udp-dns-loss-10-remote-20260603T171038Z.jsonl`
- `labs/competitive/reports/blackwire-candidate-h2plus-udp-server-metrics-20260603T171038Z.log`

| Variant | OK / 500 | Errors | p50 ms | p95 ms | p99 ms | Stale |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blackwire current Hysteria2 UDP | 446 | 54 | 1.407 | 2.200 | 2.348 | 0 |
| Blackwire candidate H2+ UDP + FEC v2 | 442 | 58 | 1.215 | 2.162 | 2.261 | 0 |
| Hysteria UDP baseline | 420 | 80 | 0.974 | 2.507 | 3.007 | 0 |

Candidate client FEC counters:

```text
fec_client_parity_packets 62
fec_client_overhead_bytes 6200
fec_client_recovered_packets 7
fec_client_stale_drops 0
fec_client_duplicate_safe_skips 0
```

Candidate server FEC counters:

```text
blackwire_fec_mode_total{mode="xor1-of-n"} 53
blackwire_fec_recovered_packets_total 1
blackwire_fec_overhead_bytes_total 5300
```

10% result:

- p99 improved vs current: `2.261ms` vs `2.348ms`.
- Errors did not improve vs current: `58` vs `54`.
- Candidate still beat Hysteria baseline on both errors and p99 in this row.
- Overhead stayed below 20%: client `6200 / 32000 = 19.4%`, server `5300 / 32000 = 16.6%`.

## Updated Acceptance Verdict

Milestone C is improved but still not fully accepted.

What is now satisfied:

- Client-side FEC recovery metrics are captured in benchmark rows.
- FEC recovery is observed on both client and server paths.
- Overhead stays below 20% in the reruns.
- The 3% row improved both error count and p99 versus Blackwire current.

What is still not satisfied:

- The 10% row improved p99 but did not improve error count versus Blackwire current.
- The p99 margin at 3% is small, not a strong >=15% acceptance margin.

Conclusion: do not keep FEC as an automatically active Milestone C path for stability-sensitive DNS traffic. The code remains available only as explicit experimental FEC modes for lab runs, while `auto` and defaults stay off. The stable runtime path should remain H2+ priority-only until FEC proves consistent error-rate improvement across the full 3-10% gate.

## Stability Decision

After the mixed 10% result, automatic FEC activation was removed:

- `fec.mode = "auto"` now resolves to `off`.
- The loss-based `modeForLoss` helper now returns `off` for auto mode.
- Tiny DNS/interactive packets are no longer implicitly rewritten from Reed-Solomon to XOR in the transport.
- FEC can still be tested by explicitly setting `fec.mode` to `xor1-of-n`, `reed-solomon`, or `raptor-like`, but it is not a stability-default path.

Stability verdict: H2+ priority-only is the safer default. FEC should not be treated as accepted or production-stable until it improves both p99 and error rate consistently under repeated 3%, 5%, and 10% loss runs.
