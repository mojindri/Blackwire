# Competitive Benchmark Summary

| File | Scenario | Variant | Status | Payload | p50 ms | p95 ms | p99 ms | req/s | Errors | Reason |
|---|---|---|---|---|---:|---:|---:|---:|---:|---|
| clean-remote-20260601T193816Z.jsonl | clean | remote-inventory | ok | 1k | 0.00 | 0.00 | 0.00 | 0.00 | 0 | wrote remote-inventory-20260601T193816Z.log |
| clean-remote-20260601T193816Z.jsonl | clean | direct-vps-native | ok | 1k | 0.20 | 0.30 | 0.60 | 35306.89 | 0 |  |
| clean-remote-20260601T193816Z.jsonl | clean | blackwire-current | ok | 1k | 0.50 | 0.90 | 1.20 | 13761.17 | 0 |  |
| clean-remote-20260601T193816Z.jsonl | clean | blackwire-candidate | skipped | 1k | 0.00 | 0.00 | 0.00 | 0.00 | 0 | no distinct BLACKWIRE_CANDIDATE_BIN configured |
| clean-remote-20260601T193816Z.jsonl | clean | xray | ok | 1k | 0.40 | 0.70 | 1.00 | 19885.49 | 0 |  |
| clean-remote-20260601T193816Z.jsonl | clean | sing-box | ok | 1k | 0.30 | 0.60 | 0.80 | 22333.95 | 0 |  |
| clean-remote-20260601T193816Z.jsonl | clean | hysteria | ok | 1k | 0.50 | 0.80 | 1.10 | 16151.83 | 0 |  |
| clean-remote-20260601T193816Z.jsonl | clean | shoes | ok | 1k | 0.30 | 0.50 | 0.80 | 24575.41 | 0 |  |

## Run Notes

- Native VPS run using server `91.107.164.107` and client `91.107.176.118`.
- Upstream: isolated native nginx on server port `18080`.
- Load: `hey`, duration 3s, concurrency 8, payload `1k`.
- Blackwire row used temporary upload of `target/linux-amd64/blackwire`; no Blackwire service was reinstalled.
- Competitor binaries installed on both VPSes before this run: Xray, sing-box, Hysteria, Shoes.
- Hysteria row used temporary self-signed TLS cert and SOCKS5 client mode over QUIC/UDP `10200`.
- Shoes row used temporary VLESS TCP server/client on `10202` with local SOCKS on `1085`.
