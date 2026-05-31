# Health Checker + Failover Example

This example shows the intended health-checking load balancer shape:

```text
local app
  -> SOCKS5 inbound
  -> routing rule selects auto-proxy balancer
  -> health state and adaptive scoring filter weak outbounds
  -> adaptive strategy chooses the best scored profile
  -> target site
```

The balancer watches `primary-vless` and `backup-ss2022`. When health checks mark
one path dead, new connections should fail over to the other path. The named
`profiles` in this example are balancer profiles mapped to outbound tags; they
are separate from the top-level Blackwire operating profile such as
`profile: "fast"`. If both paths are dead, the balancer falls back to the first
configured outbound so failures stay explicit instead of disappearing silently.

## Validation

Config syntax only:

```sh
cargo run -q -p blackwire -- test -c examples/health-failover/config.json
```

Runtime failover proof (recommended):

```sh
cargo test -p integration-tests --test e2e_health_failover health_failover_routes_to_backup_when_primary_unhealthy
make -C labs/realistic health-failover
```

See [labs/realistic/health-failover/README.md](../../labs/realistic/health-failover/README.md).

Author: @moji.ndr
