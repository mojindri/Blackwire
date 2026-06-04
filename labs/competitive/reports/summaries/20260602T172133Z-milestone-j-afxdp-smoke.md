# Milestone J AF_XDP smoke

- Timestamp: `2026-06-02T17:21:33Z`
- Branch: `feature/log_readme`
- Commit: `76df02e1`
- VPS: `<server-host>`
- Kernel: `Linux 7.0.0-15-generic`
- Interface: `eth0`

## Validation

Executed privileged Linux smoke:

```bash
TMPDIR=/var/tmp \
CARGO_TARGET_DIR=/var/tmp/blackwire-afxdp-target \
BLACKWIRE_AF_XDP_IFACE=eth0 \
cargo test -p blackwire-transport --features priv-test --test tun_priv \
  af_xdp_backend_opens_on_configured_interface -- --exact --nocapture
```

Result:

- `af_xdp_backend_opens_on_configured_interface ... ok`
- AF_XDP backend successfully opened a real socket, UMEM, and rings on `eth0` queue `0`.

## Notes

- First VPS attempt failed due `/tmp` tmpfs exhaustion during Rust dependency build; rerun succeeded with `TMPDIR=/var/tmp`.
- This summary records backend bring-up validation only.
- Performance acceptance is still pending a Linux-only benchmark proving benefit versus the current default path.
