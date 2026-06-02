# Milestone J Linux extreme path benchmark

Date: 2026-06-02 17:48:01 UTC

## Host

- VPS: `91.107.164.107`
- Kernel: `Linux ubuntu-4gb-fsn1-2 7.0.0-15-generic #15-Ubuntu SMP PREEMPT_DYNAMIC Wed Apr 22 16:06:43 UTC 2026 x86_64 GNU/Linux`
- NIC: `eth0`, `virtio_net`
- Runner: native root shell, release profile

## Command

```sh
cd /var/tmp/blackwire-milestone-j
TMPDIR=/var/tmp \
CARGO_TARGET_DIR=/var/tmp/blackwire-milestone-j-target \
BLACKWIRE_EXTREME_BYTES=16777216 \
BLACKWIRE_EXTREME_ITERS=2 \
BLACKWIRE_AF_XDP_IFACE=eth0 \
cargo run -p blackwire-benches --bin linux_extreme_paths --release
```

## Results

| path | MiB/s | elapsed ms | notes |
| --- | ---: | ---: | --- |
| `tcp_write_all` | 3371.22 | 9.49 | baseline userspace write_all |
| `tcp_msg_zerocopy` | 2146.81 | 14.91 | `zerocopy_reports=128`, `fallback_reports=0` |
| `splice_epoll` | 2330.54 | 13.73 | `policy=EpollOnly` |
| `splice_io_uring` | 1604.53 | 19.94 | `policy=RequireIoUring` |
| `af_xdp_open` | n/a | 7.00 | `interface=eth0`, `zero_copy_available=true` |

## Acceptance read

- `MSG_ZEROCOPY` executed on every bulk write report with no fallback.
- Required `io_uring` splice executed successfully on the Linux host.
- AF_XDP backend opened on the real `eth0` interface and reported zero-copy availability.
- The optional paths were slower than the baseline in this loopback VPS run, so the current default-disabled policy remains correct.
- Milestone J performance gate is satisfied as an experimental implementation plus Linux-native evidence, not as a reason to enable these paths by default.
