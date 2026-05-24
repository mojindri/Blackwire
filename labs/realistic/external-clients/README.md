# External Client Compatibility Lab

This lab checks real external clients against `proxy-rs` server inbounds:

```text
Xray or sing-box client -> proxy-rs server -> target-http
```

It is intentionally separate from `vps-test`, which checks `proxy-rs` client to
`proxy-rs` server. Passing this lab is the signal that real apps built on
Xray/sing-box behavior are compatible with the server side.

## Commands

From `labs/realistic`:

```sh
make external-clients-docker
make external-clients-report
```

For the two-VPS promotion gate:

```sh
SSH_SERVER=1.2.3.4 SSH_CLIENT=5.6.7.8 SSH_KEY=~/.ssh/id_hetzner make external-clients-vps
make external-clients-report
```

The VPS runner assumes the normal server/client setup already ran. It does not
install Docker or packages. It starts one `/usr/local/bin/proxy-rs` inbound at a
time on the server VPS, runs Xray/sing-box Docker clients on the client VPS, and
writes full logs under `labs/realistic/reports/external-clients-vps/`.

The runner keeps console output compact and writes full logs under:

```text
labs/realistic/reports/external-clients/
```

## Matrix Order

The matrix runs inbound compatibility in this order:

1. Trojan over TLS
2. VLESS TCP
3. VLESS over WebSocket
4. VMess over gRPC
5. Shadowsocks 2022
6. Hysteria2
7. VLESS REALITY

Xray and sing-box are both mandatory where the client supports the protocol
cleanly. Hiddify remains a manual validation target using generated import
artifacts after the automated clients pass.

For every supported positive case, the lab also renders a negative-auth variant
with the wrong UUID/password/shortId. Those cases must fail to fetch the target;
otherwise the report marks them as accepted and fails the run.
