# Milestone I - TUN VPS Rerun

Date: 2026-06-02 14:53:39 UTC

## Context

After manual reboot, client VPS `91.107.176.118` was reachable again and its route table was clean.

The TUN benchmark harness was updated to run probes inside the same SSH session as the active TUN runtime. This avoids opening new SSH sessions while TUN policy routing is active.

## Runs

Failed pre-fix run:

```text
labs/competitive/reports/tun-remote-20260602T144103Z.jsonl
```

Single-session runner run:

```text
labs/competitive/reports/tun-remote-20260602T144635Z.jsonl
```

## UDP Result

The UDP rows completed for both Blackwire and sing-box.

| Variant | UDP p99 | Status |
| --- | ---: | --- |
| Blackwire candidate TUN | 1.441 ms | ok |
| sing-box TUN | 0.352 ms | ok |

Acceptance threshold:

```text
Blackwire UDP p99 <= sing-box UDP p99 * 1.15
```

Observed ratio:

```text
1.441 / 0.352 = 4.09x
```

UDP acceptance failed.

## TCP Result

TCP acceptance was not produced.

Rows in `tun-remote-20260602T144635Z.jsonl`:

- Blackwire candidate TUN TCP: failed with `curl: (52) Empty reply from server`
- sing-box TUN TCP: failed with `curl: (7) Failed to connect ... Could not connect to server`

Additional checks showed that client VPS `91.107.176.118` refuses outbound TCP to several tested targets even with no TUN active:

- `91.107.164.107:22`
- `91.107.164.107:18080`
- `1.1.1.1:443`
- `google.com:443`
- `github.com:443`

The server VPS `91.107.164.107` has normal outbound TCP. This means the planned client-side TCP benchmark cannot satisfy the acceptance gate until the VPS/network/firewall policy for outbound TCP on `91.107.176.118` is corrected, or the benchmark roles are moved to a host with normal TCP egress and a reachable upstream target.

## Acceptance Status

Milestone I is not accepted.

Reasons:

- UDP p99 acceptance failed: Blackwire `4.09x` sing-box, required `<=1.15x`.
- TCP throughput and CPU acceptance were not measurable on the current client VPS TCP path.

## Follow-Up

- Investigate Blackwire Linux TUN UDP latency overhead vs sing-box.
- Fix/replace the client VPS TCP egress path before rerunning TCP throughput and CPU acceptance.
- Keep using the single-session TUN runner to avoid route-induced SSH lockout during future runs.
