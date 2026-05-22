# Phase 8 mKCP + VLESS Example

This example shows the intended mKCP transport shape:

```text
client app
  -> local SOCKS5 inbound
  -> VLESS outbound
  -> mKCP over UDP
  -> server mKCP listener
  -> VLESS inbound
  -> Freedom outbound
  -> target site
```

mKCP is useful when the link is UDP-friendly but TCP performs badly because of
loss or unstable latency. The example keeps the protocol as VLESS and swaps the
transport from plain TCP/WebSocket/gRPC to `network: "kcp"`.

Current caveat: the schema and low-level mKCP transport primitives exist, but
the main instance transport stack does not yet apply `network: "kcp"` to VLESS
inbounds and outbounds. Treat these files as validated templates for the next
runtime wiring step.

Validate:

```sh
cargo run -q -p proxy-rs -- test -c examples/phase8-mkcp-vless/client.json
cargo run -q -p proxy-rs -- test -c examples/phase8-mkcp-vless/server.json
```

Author: @moji.ndr
