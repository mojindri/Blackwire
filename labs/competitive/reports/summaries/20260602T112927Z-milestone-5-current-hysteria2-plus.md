# Milestone 5 / Hysteria2 UDP-DNS Prep Test Summary

Date: 2026-06-02

Scope:
- Hysteria2 datagram policy wiring and H2-plus mode fields in config.
- Transport exports + core parser adaptation.
- Integration test compatibility for updated Hysteria2 UDP session config shape.
- Static/build/test validation for the milestone slice.

Execution (local, non-benchmark):
- `cargo check -q`
- `cargo fmt --all`
- `cargo test -p blackwire-config datagram_and_fec_policy_deserialise -- --nocapture`
- `cargo test -p blackwire-config fec_auto_policy_tracks_loss_and_packet_class -- --nocapture`
- `cargo test -p blackwire-transport fec_recovers -- --nocapture`
- `cargo test -p blackwire-core --test reload_listeners -- --nocapture`
- `cargo test -p integration-tests --test e2e_hysteria2_udp -- --nocapture`
- `ssh -i id_hetzner root@<server-host> 'hostname'` (baseline SSH reachability check before running against remote milestone scripts)
- `cargo test -p blackwire-transport --lib -- --nocapture`

Results:
- `cargo check -q` passed after adding `blackwire_transport` exports and fixing parser struct field wiring.
- `cargo fmt --all` completed with no output.
- `datagram_and_fec_policy_deserialise` passed.
- `fec_auto_policy_tracks_loss_and_packet_class` passed.
- `fec_recovers` passed (XOR and Reed-Solomon restore one missing datagram).
- `reload_listeners` targeted test set passed (including datagram/FEC change restart sensitivity).
- `e2e_hysteria2_udp` passed after adding `datagram_policy: DatagramPolicy::default()` to test `Hysteria2ClientConfig`.
- Local config and UDP/FEC integration are now internally consistent with new fields and parsing.
- `blackwire_transport --lib` run returned all 103 tests passing, including new Hysteria2 pacer and window-profile tests.

Open blockers for complete Milestone 5 acceptance:
- Dedicated lossy UDP/DNS benchmark rows are still outstanding; performance gate still requires UDP/DNS p99 proof at 3–10% loss with overhead cap checks against official Hysteria.
