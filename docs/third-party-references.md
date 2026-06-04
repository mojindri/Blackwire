# Third-Party References

Blackwire is an independent implementation. These projects were studied for
compatibility and performance behavior:

| Project | License | Reference use |
| --- | --- | --- |
| Xray-core | MPL-2.0 | VLESS, REALITY, Vision compatibility behavior and benchmark comparison |
| sing-box | GPL-3.0-or-later | Routing, connection-path, and TUN performance comparison |
| Hysteria | MIT | Hysteria2 compatibility and bad-network performance comparison |
| Shoes | MIT | Rust relay-loop performance comparison |

No source code from these projects is copied into Blackwire unless a future file
explicitly carries the required license header and review note.

For implementation work:

- Prefer protocol specifications, wire captures, interop tests, and clean-room
  behavior descriptions.
- Do not paste or mechanically translate GPL/MPL implementation code.
- If an MPL-derived file is ever introduced, keep it isolated and preserve the
  required MPL-2.0 notice.
