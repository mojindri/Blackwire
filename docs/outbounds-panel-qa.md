# Outbounds Panel QA Result

## Run Summary
- Panel URL: `http://127.0.0.1:18180/`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-results-qa-mpyl0ro7.json`
- Passed: 18
- Failed: 1
- Skipped: 2

## Results
- PASS: Freedom/default - qa-mpyl0ro7-freedom persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/TCP - qa-mpyl0ro7-vless-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/WS - qa-mpyl0ro7-vless-ws persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/gRPC - qa-mpyl0ro7-vless-grpc persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/HTTPUpgrade - qa-mpyl0ro7-vless-httpupgrade persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/SplitHTTP - qa-mpyl0ro7-vless-splithttp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- SKIPPED: VLESS/KCP - kcp transport selector could not be reached cleanly
- SKIPPED: VLESS/QUIC - quic transport selector could not be reached cleanly
- PASS: VLESS/TCP+TLS - qa-mpyl0ro7-vless-tls persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VLESS/TCP+REALITY - qa-mpyl0ro7-vless-reality persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: VMess/TCP - qa-mpyl0ro7-vmess-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: Trojan/TCP - qa-mpyl0ro7-trojan-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: Shadowsocks/TCP - qa-mpyl0ro7-shadowsocks-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: Hysteria2/TCP - qa-mpyl0ro7-hysteria2-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyl0ro7-config.json
- PASS: Advanced JSON preserve - qa-mpyl0ro7-advanced-preserve preserved unknown keys while structured fields changed
- PASS: Invalid UUID guard - invalid UUID blocked before save
- PASS: Invalid JSON guard - malformed advanced JSON blocked before save
- PASS: Missing password guard - trojan password guard shown
- PASS: Delete flow - qa-mpyl0ro7-delete removed from config output
- PASS: Disabled outbound omission - qa-mpyl0ro7-toggle omitted from generated config when disabled
- FAILED: No-enabled fallback - unexpected outbounds array: [{"protocol":"vless","settings":{"_qaKeepSettings":{"marker":"keep-me","nested":{"ok":true}},"address":"127.0.0.1","port":29740,"users":[{"id":"b6d9a819-7d1c-4a68-8dd5-2d64c8f0dd91"}]},"streamSettings":{"_qaKeepTransport":{"marker":"preserve","nested":{"value":1}},"network":"ws","security":"none","wsSettings":{"headers":{"Host":"changed.example.com"},"path":"/adv"}},"tag":"qa-mpyl0ro7-advanced-preserve"},{"protocol":"vless","settings":{"address":"127.0.0.1","port":29760,"users":[{"id":"a5d2d87d-5c39-44cb-9c48-9b5a1e8cfd6f"}]},"streamSettings":{"network":"tcp","security":"none"},"tag":"qa-mpyl0ro7-toggle"}]

## Routing
- PASS: Adaptive routing workflow - routing balancer references 63 enabled outbounds

## Skipped Cases
- VLESS/KCP: kcp transport selector could not be reached cleanly
- VLESS/QUIC: quic transport selector could not be reached cleanly

## Notes
- The live panel was exercised through the structured outbound editor and Advanced Config routing workflow.
- Disk assertions were performed against the generated `config.json` after successful saves.

