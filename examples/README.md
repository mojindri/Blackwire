# Database seed examples

Blackwire no longer ships deployable JSON configuration examples. MySQL is the
only configuration source of truth, so examples are named relational presets:

```bash
blackwire db seed socks-local
blackwire db seed vless-local
blackwire db seed trojan-local
blackwire db seed shadowsocks-local
```

Each command creates an immutable desired revision that Blackwire validates and
applies automatically. Credential-bearing presets print their generated
credential and subscription token once.

Client links remain available from each user's Copy subscription action in
Black UI. These links are derived from MySQL state and can be pasted into
Hiddify; they are not server configuration exports.
