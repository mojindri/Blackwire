# Milestone C Remote Vision Direct-Copy Benchmark

Date: 2026-06-01

## Command

```sh
COMPETITIVE_MODE=remote \
COMPETITIVE_DURATION=3 \
COMPETITIVE_CONCURRENCY=8 \
COMPETITIVE_PAYLOADS=1k \
BLACKWIRE_CANDIDATE_BIN=$PWD/target/linux-amd64/blackwire-candidate-vision-direct \
make competitive-expensive competitive-report
```

## Environment

- Server VPS: `<server-host>`
- Client VPS: `<client-host>`
- Candidate binary: native Linux x86_64 release build at `target/linux-amd64/blackwire-candidate-vision-direct`
- Upstream: `https://www.microsoft.com`
- REALITY fallback/camouflage path: nginx `stream` listener on server `127.0.0.1:18443` forwarding to `www.microsoft.com:443`
- Client load generator: `hey` through Xray HTTP proxy inbounds
- Raw result file: `labs/competitive/reports/expensive-remote-20260601T211801Z.jsonl`

## Results

| Variant | Status | Requests/sec | p50 ms | p90 ms | p95 ms | Errors | Notes |
|---|---|---:|---:|---:|---:|---:|---|
| `direct-vps-native` | failed | 6.51 | 42.40 | 66.70 | 82.10 | 8 | External HTTPS direct path hit client timeouts during this short run. |
| `blackwire-current-vision` | failed | 400.60 | 0.00 | 0.00 | 0.00 | 1215 | Raw client log shows `tls: bad record MAC`; this is the pre-fix behavior. |
| `blackwire-candidate-vision` | ok | 4.98 | 1056.20 | 2188.60 | 5085.30 | 0 | 37 successful HTTP 200 responses through Xray client -> Blackwire REALITY Vision server. |
| `xray-vision` | ok | 1.55 | 5086.50 | 0.00 | 0.00 | 0 | 8 successful HTTP 200 responses through Xray client -> Xray REALITY Vision server. |

## Conclusion

Milestone C Vision server compatibility is satisfied for the remote Xray-client gate: the candidate accepts real Xray VLESS+REALITY+Vision traffic and relays HTTPS without client TLS MAC failures. The candidate outperformed the Xray Vision baseline in this short run (`4.98` req/s vs `1.55` req/s) while preserving zero errors.

The run also captured the old Blackwire behavior (`blackwire-current-vision`) failing with `tls: bad record MAC`, confirming the candidate changed the failure mode rather than the harness hiding it.
