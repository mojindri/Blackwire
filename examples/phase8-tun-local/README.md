# Phase 8 Linux TUN Mode Example

This example shows the intended Linux TUN interception shape:

```text
applications
  -> Linux policy routing / TUN interface
  -> local redirect ports
  -> SOCKS5 or HTTP inbound
  -> routing rules
  -> proxy or direct outbound
```

The TUN defaults match the transport helper defaults: `proxy-tun`, address
`198.18.0.1`, route table policy mark `0x1234`, TCP redirect port `7890`, and
DNS redirect port `5300`. Linux setup requires root or the needed network
capabilities because it creates a TUN device and installs `ip rule`, `ip route`,
and `iptables` rules.

Current caveat: the Linux TUN helper module is present in `proxy-transport`, but
there is not yet a top-level `tun` schema field or an instance startup path that
creates the TUN device from this JSON file. The `tun` block is included as the
expected deployment shape and is currently ignored by config deserialization.

Validate:

```sh
cargo run -q -p proxy-rs -- test -c examples/phase8-tun-local/config.json
```

Author: @moji.ndr
