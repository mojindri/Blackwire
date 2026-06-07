# License Policy

Blackwire is an independent MIT-licensed implementation.

The repository may study public behavior, protocol specifications, benchmark
results, and interoperability requirements from other proxy projects, but source
code must not be copied into MIT Blackwire unless the derived file is explicitly
marked and reviewed for license compatibility.

Rules:

- sing-box GPL code must not be copied into MIT Blackwire.
- Xray MPL-derived code must be isolated and marked MPL-2.0 if it is ever used.
- Third-party-derived files require an in-file license header and a review note.
- Compatibility behavior should be reimplemented from protocol behavior, tests,
  and documentation rather than mechanically translating source code.
- Dependency licenses are checked with `cargo deny`.
- Known security advisories are checked with `cargo audit`.

Allowed routine inputs:

- RFCs and standards documents.
- Public protocol behavior observed through interop tests.
- Benchmark results and performance profiles.
- Project documentation and configuration examples, subject to their licenses.

Disallowed inputs:

- Raw copied GPL source files.
- Raw copied MPL source files without MPL isolation and headers.
- Mechanical translations of GPL/MPL implementation files into MIT modules.
- Vendored third-party repository snapshots without a recorded license decision.

## Third-Party References

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
