# Blackwire client application

`blackwire-client` owns full-device capture and other device-side behavior.
The server command, `blackwire run`, no longer creates TUN interfaces, changes
host routes, or reads TUN/FakeIP settings from the MySQL control plane.

The protocol, relay, DNS/FakeIP, and platform TUN implementations remain shared
with the rest of Blackwire. Only their lifecycle and configuration ownership
are client-specific.

## Run

Start with [examples/client-direct.json](../examples/client-direct.json), then
replace its direct outbound with the remote VLESS, Trojan, Shadowsocks 2022,
Hysteria2, or TUIC outbound used by the device.

```sh
cargo build -p blackwire-client --release
sudo ./target/release/blackwire-client --config examples/client-direct.json
```

Official platform archives include `blackwire-client` alongside the server binary.

Elevated privileges are required to create the interface and install routes.
On macOS and Windows, set `tun.outboundInterface` to the physical interface used
for protected proxy egress. Windows may also require `tun.wintunFile`.

## Configuration contract

- A top-level `tun` section is required.
- A loopback SOCKS inbound must listen on the same port as `tun.redirectPort`.
- FakeIP belongs under `dns.fake_ip` in this client file.
- TUN settings are file-owned and are not stored by Black UI or the server
  database.
- Migration archives prior database values in `archived_client_tun_settings`
  and `archived_client_fake_ip_settings` for manual migration or rollback.
- The client rejects invalid configuration before changing routes.
- Ctrl-C signals the TUN runtime to remove routes before process exit.

The initial client boundary deliberately uses a local typed JSON file. A future
desktop/mobile shell can own this same configuration without coupling device
permissions or routes back into the server control plane.
