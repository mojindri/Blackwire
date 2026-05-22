# Phase 8 Health Checker + Failover Example

This example shows the intended health-checking load balancer shape:

```text
local app
  -> SOCKS5 inbound
  -> routing rule selects auto-proxy balancer
  -> health state filters dead outbounds
  -> latency strategy chooses the fastest alive outbound
  -> target site
```

The balancer watches `primary-vless` and `backup-ss2022`. When health checks mark
one path dead, new connections should fail over to the other path. If both paths
are dead, the balancer falls back to the first configured outbound so failures
stay explicit instead of disappearing silently.

Current caveat: the balancer and health-check modules are present in
`proxy-app`, and the config schema accepts `routing.balancers`, but the main
instance still needs runtime registration of balancer handlers and health-check
tasks before this becomes a fully live failover setup. Treat this as the config
template for that wiring.

Validate:

```sh
cargo run -q -p proxy-rs -- test -c examples/phase8-health-failover/config.json
```

Author: @moji.ndr
