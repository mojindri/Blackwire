# User Guide

This guide covers the normal operator path: install, configure, run, use Black
UI, troubleshoot, and understand advanced switches.

> Pre-production warning
>
> Blackwire pre-1.0 releases are for personal testing, labs, and tightly
> controlled deployments. They are not a promise of production readiness. A
> valid config only proves Blackwire accepts the file; it does not certify the
> selected protocol, transport, panel workflow, or platform for production use.
> Read [release.md](release.md) before exposing a service to users.

## Install

Linux release assets currently support:

- Linux `x86_64` / `amd64`
- Linux `aarch64` / `arm64`

Basic install:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 bash
```

The service is not started by default. Blackwire needs a valid config before it
can run.

Install with an existing config:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 CONFIG_PATH=/path/to/config.json START_SERVICE=1 bash
```

Generate a VLESS REALITY VPS config:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 SETUP=reality PUBLIC_HOST=example.com START_SERVICE=1 bash
```

Generate a domain + nginx + TLS setup:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 SETUP=domain DOMAIN=proxy.example.com PROXY_PATH=/secret-path INSTALL_NGINX=1 INSTALL_CERTBOT=1 START_SERVICE=1 bash
```

For domain setup, point DNS at the VPS first and open `tcp/80` and `tcp/443` in
the provider firewall.

Generated VPS setup writes client details here:

```sh
sudo cat /etc/blackwire/client-info.txt
```

## Black UI

Black UI reads runtime capabilities from the companion server. QUIC transport
and TUIC v5 are available in the inbound and outbound protocol pickers as
supported UDP-based options; throughput still depends on the client, firewall,
NAT, and network path.

Black UI is the companion web panel for controlled deployments and operator
testing. Do not expose it directly to the public internet.

Install the panel:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 INSTALL_BLACK_UI=1 bash
```

Install Blackwire with domain setup and Black UI:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 SETUP=domain DOMAIN=proxy.example.com PROXY_PATH=/secret-path INSTALL_NGINX=1 INSTALL_CERTBOT=1 INSTALL_BLACK_UI=1 START_SERVICE=1 bash
```

Defaults:

```text
/var/lib/black-ui                         Black UI data
127.0.0.1:18080                           Black UI listen address
/usr/local/share/black-ui/frontend/dist   Black UI static frontend
/etc/blackwire/config.json                Blackwire config
127.0.0.1:62789                           Blackwire gRPC API
```

With domain setup, the installer reverse-proxies Black UI at `/panel/`.

In the inbound TLS security section, Black UI can generate a self-signed
certificate and key on the server for IP-only or lab deployments. Generated
files are stored under `/etc/blackwire/certs` with service-readable permissions,
and the panel fills the TLS certificate/key paths automatically. Clients still
need to trust that certificate or use their explicit insecure/skip-verify
setting. For public domain deployments, prefer a normal CA-issued certificate.

Useful commands:

```sh
sudo systemctl status black-ui --no-pager
sudo journalctl -u black-ui -f
sudo systemctl restart black-ui
```

Keep the panel bound to localhost and expose it through authenticated HTTPS
reverse proxy. Do not bind `BLACK_UI_LISTEN=0.0.0.0:18080` on a public VPS
without an external access-control layer.

## Operate The Service

Common paths:

```text
/usr/local/bin/blackwire       binary
/etc/blackwire/config.json     main config
/etc/blackwire/client-info.txt generated client connection hints
/var/lib/blackwire             service working directory
/etc/systemd/system/blackwire.service systemd unit
```

Service commands:

```sh
blackwire version
blackwire test -c /etc/blackwire/config.json
sudo systemctl status blackwire --no-pager
sudo systemctl start blackwire
sudo systemctl stop blackwire
sudo systemctl restart blackwire
sudo systemctl reload blackwire
sudo journalctl -u blackwire -f
```

For domain setup:

```sh
sudo nginx -t
sudo systemctl status nginx --no-pager
sudo systemctl restart nginx
sudo journalctl -u nginx -n 100 --no-pager
```

Upgrade:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 ACTION=upgrade bash
```

Uninstall but keep config and state:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | ACTION=uninstall bash
```

Remove config and state too:

```sh
curl -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | ACTION=uninstall REMOVE_CONFIG=1 bash
```

## Configure

Blackwire uses native JSON config. It does not import Xray or sing-box config
files directly.

Validate before applying changes:

```sh
blackwire test -c /etc/blackwire/config.json
sudo systemctl reload blackwire
```

Smallest useful server shape:

```json
{
  "log": { "level": "info", "json": false },
  "inbounds": [
    {
      "tag": "vless-in",
      "protocol": "vless",
      "listen": "0.0.0.0",
      "port": 443,
      "settings": {
        "clients": [
          { "id": "00000000-0000-0000-0000-000000000000", "email": "user@example.local" }
        ]
      }
    }
  ],
  "outbounds": [
    { "tag": "freedom", "protocol": "freedom" }
  ],
  "routing": {
    "rules": [
      { "outboundTag": "freedom" }
    ]
  }
}
```

Generate a UUID:

```sh
blackwire uuid
```

Common sections:

- `inbounds`: listeners that accept client traffic.
- `outbounds`: destinations Blackwire can send traffic to.
- `routing`: rules that choose an outbound.
- `dns`: resolver, FakeIP, and domain strategy settings.
- `tun`: transparent proxy runtime settings.
- `metricsAddr`: Prometheus metrics listen address.
- `api`: local gRPC Handler/Stats API settings.
- `profile`: compatibility or fast operating profile.

Freedom outbounds can control how direct destinations are resolved and dialed:

```json
{
  "tag": "freedom",
  "protocol": "freedom",
  "settings": {
    "denyLoopback": true,
    "domainStrategy": "PreferIPv4",
    "rejectIpv6Literal": false
  }
}
```

`PreferIPv4` keeps IPv6 as a fallback for domain destinations. `UseIPv4` is
stricter and removes IPv6 domain results. Keep `rejectIpv6Literal` disabled for
normal clients because DNS stacks may send already-resolved IPv6 literal
destinations. Enable it only when you intentionally want to fail those literal
IPv6 dials before touching the network.

For detailed field explanations, read [08-config-for-dummies.md](08-config-for-dummies.md).
For support labels, read [release.md](release.md) and
[feature-matrix.md](feature-matrix.md).

Useful examples:

- [VLESS Client/Server](../examples/vless-client-server/README.md)
- [REALITY Client/Server](../examples/reality-client-server/README.md)
- [Hysteria2 Client/Server](../examples/hysteria2-client-server/README.md)
- [VLESS + WebSocket Local](../examples/vless-ws-local/README.md)
- [DNS + FakeIP Routing](../examples/dns-fakeip-routing/README.md)
- [TUN Local](../examples/tun-local/README.md)

## Troubleshooting

Hysteria2 works best when the client UDP path is stable. If a third-party client
works for a few minutes and then stalls in global/TUN mode, first isolate the
server with a SOCKS/system-proxy style profile. If that path is stable, check
the client TUN/FakeIP settings: avoid a jumbo TUN MTU such as `9000`, try `1500`
or `1280`, and disable FakeIP DNS mode if the client logs show FakeIP rejecting
PTR or HTTPS queries.

Service does not start:

```sh
sudo systemctl status blackwire --no-pager
sudo journalctl -u blackwire -n 100 --no-pager
blackwire test -c /etc/blackwire/config.json
```

Domain setup fails:

- DNS A/AAAA record points to the VPS.
- Provider firewall allows `tcp/80` and `tcp/443`.
- Server firewall allows `tcp/80` and `tcp/443`.
- No other service is already bound to `80` or `443`.
- `DOMAIN=...` is set.
- You used `PROXY_PATH=/secret-path`, not shell `PATH`.

Check:

```sh
sudo ss -ltnp | grep -E ':80|:443'
sudo nginx -t
sudo systemctl status nginx --no-pager
```

Black UI does not open:

```sh
sudo systemctl status black-ui --no-pager
sudo journalctl -u black-ui -n 100 --no-pager
sudo ss -ltnp | grep 18080
```

Users cannot connect:

- check Blackwire logs
- check provider firewall and server firewall
- check the client UUID/password from `/etc/blackwire/client-info.txt`
- check transport/security settings
- for REALITY, clients need the public key, not the private key
- for REALITY, the client SNI must match a server-side `serverNames` entry
- for domain setup, clients need the configured `PROXY_PATH`

If GitHub downloads fail, try IPv4:

```sh
curl -4 -fsSL https://raw.githubusercontent.com/mojindri/Blackwire/v0.1.40/scripts/install.sh \
  | VERSION=v0.1.40 bash
```

## Advanced

Fast Profile is a narrower latency-first mode for controlled deployment
experiments:

```sh
blackwire run -c /etc/blackwire/config.json --profile fast
```

Read [fast-profile.md](fast-profile.md) before enabling it.

Metrics endpoints, when configured:

- `/metrics`
- `/healthz`
- `/readyz`
- `/version`

Keep metrics, gRPC APIs, and admin panels on localhost or behind explicit access
control.

Validation and evidence docs:

- [11-testing.md](11-testing.md)
- [test-workflows.md](test-workflows.md)
- [performance-evidence.md](performance-evidence.md)
- [latency-lab.md](latency-lab.md)
- [release.md](release.md)
